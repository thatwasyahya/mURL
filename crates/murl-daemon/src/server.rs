//! The request handler and the Unix-socket server.
//!
//! [`handle_request`] is transport-free and therefore fully testable: the
//! whole security-relevant path (resolve → policy → consent → dispatch)
//! runs without a socket. The listener is a thin loop around it.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use murl_core::dispatch::{execute, Approval, Launcher, OpenerConfig};
use murl_core::error::Result;
use murl_core::murl::Murl;
use murl_core::policy::Policy;
use murl_core::resolver::Resolver;

use crate::consent_ui::{self, ConsentUi};
use crate::protocol::{parse_request, Request, Response, MAX_LINE_BYTES, PROTOCOL_VERSION};

/// Borrowing a `Resolver` requires several collaborators to outlive it, so
/// the daemon takes a factory that lends one for the duration of a closure.
pub type WithResolver<'a> = dyn Fn(&mut dyn FnMut(&Resolver<'_>) -> Result<()>) -> Result<()> + 'a;

/// Everything a request needs, assembled once at startup.
pub struct Context<'a> {
    pub with_resolver: &'a WithResolver<'a>,
    pub policy: Policy,
    pub opener: OpenerConfig,
    pub launcher: &'a dyn Launcher,
    pub consent: &'a dyn ConsentUi,
    pub limits: murl_core::Limits,
    pub started_at: u64,
    pub socket: String,
    pub activations: AtomicU64,
    pub version: &'static str,
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("socket", &self.socket)
            .field("version", &self.version)
            .field("activations", &self.activations)
            .finish_non_exhaustive()
    }
}

/// Handle one request, returning every response line it produces.
pub fn handle_request(ctx: &Context<'_>, request: Request, now: u64) -> Vec<Response> {
    match request {
        Request::Ping { .. } => vec![Response::Pong {
            protocol: PROTOCOL_VERSION,
            version: ctx.version.to_owned(),
        }],
        Request::Status { .. } => vec![Response::Status {
            version: ctx.version.to_owned(),
            uptime_secs: now.saturating_sub(ctx.started_at),
            activations: ctx.activations.load(Ordering::Relaxed),
            socket: ctx.socket.clone(),
        }],
        Request::Resolve { murl, .. } => resolve_plan(ctx, &murl),
        Request::Activate { murl, only, .. } => activate(ctx, &murl, &only),
    }
}

fn resolve_plan(ctx: &Context<'_>, murl: &str) -> Vec<Response> {
    let parsed = match Murl::parse(murl) {
        Ok(m) => m,
        Err(e) => return vec![Response::error("parse", e.to_string())],
    };
    let mut plan = None;
    let run = (ctx.with_resolver)(&mut |resolver: &Resolver<'_>| {
        let mut resolution = resolver.resolve(&parsed)?;
        resolution.apply_policy(&ctx.policy);
        plan = Some(resolution.to_json());
        Ok(())
    });
    match run {
        Ok(()) => vec![Response::Plan {
            resolution: plan.expect("resolver ran"),
        }],
        Err(e) => vec![Response::error("resolve", e.to_string())],
    }
}

fn activate(ctx: &Context<'_>, murl: &str, only: &[String]) -> Vec<Response> {
    let parsed = match Murl::parse(murl) {
        Ok(m) => m,
        Err(e) => return vec![Response::error("parse", e.to_string())],
    };

    let mut responses = Vec::new();
    let run = (ctx.with_resolver)(&mut |resolver: &Resolver<'_>| {
        let mut resolution = resolver.resolve(&parsed)?;
        resolution.apply_policy(&ctx.policy);
        responses.push(Response::Plan {
            resolution: resolution.to_json(),
        });

        // Consent happens here, inside the daemon: a client asking to
        // activate is asking for a dialog, never for a launch.
        let (request, slots) = consent_ui::prepare(&resolution, only);
        let granted = ctx.consent.ask(&request);
        let approvals = consent_ui::apply(slots, &granted);

        let ids = |want_granted: bool| -> Vec<String> {
            resolution
                .resources
                .iter()
                .zip(&approvals)
                .filter(|(_, a)| {
                    if want_granted {
                        matches!(a, Approval::Approved)
                    } else {
                        matches!(a, Approval::Denied(_))
                    }
                })
                .map(|(pr, _)| pr.resource.id.clone())
                .collect()
        };
        responses.push(Response::Consent {
            granted: ids(true),
            denied: ids(false),
        });

        let report = execute(
            &resolution,
            &approvals,
            &ctx.opener,
            ctx.launcher,
            &ctx.limits,
        )?;
        ctx.activations.fetch_add(1, Ordering::Relaxed);
        responses.push(Response::Outcome {
            report: report.to_json(),
        });
        Ok(())
    });

    if let Err(e) = run {
        responses.push(Response::error("activate", e.to_string()));
    }
    responses
}

/// Serve one already-accepted connection: read request lines, write
/// response lines. Bounded by [`MAX_LINE_BYTES`] and by `max_requests`
/// (threat D-3).
pub fn serve_connection<R: Read, W: Write>(
    ctx: &Context<'_>,
    reader: R,
    mut writer: W,
    now: u64,
    max_requests: usize,
) -> std::io::Result<()> {
    let budget = (MAX_LINE_BYTES as u64 + 1).saturating_mul(max_requests as u64);
    let mut reader = BufReader::new(reader.take(budget));
    let mut served = 0usize;
    let mut line = String::new();
    while served < max_requests {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }
        let responses = match parse_request(trimmed) {
            Ok(request) => handle_request(ctx, request, now),
            Err(error) => vec![error],
        };
        for response in responses {
            writer.write_all(response.to_line().as_bytes())?;
        }
        writer.flush()?;
        served += 1;
    }
    Ok(())
}

#[cfg(unix)]
pub use unix_server::run;

#[cfg(unix)]
mod unix_server {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;

    use murl_core::error::{Error, Result};
    use murl_core::time::Clock;

    use super::{serve_connection, Context};
    use crate::socket;

    /// Bind the socket and serve until interrupted.
    ///
    /// Refuses to clobber a live socket (threat D-5) and binds 0600 inside
    /// a 0700 directory (threat D-1).
    pub fn run(ctx: &Context<'_>, path: &Path, clock: &dyn Clock) -> Result<()> {
        socket::prepare_socket_dir(path)?;

        if path.exists() {
            // A live daemon owns this socket; a dead one left a stale file.
            match UnixStream::connect(path) {
                Ok(_) => {
                    return Err(Error::Denied(format!(
                        "another daemon is already listening on {}",
                        path.display()
                    )))
                }
                Err(_) => std::fs::remove_file(path)?,
            }
        }

        let listener = UnixListener::bind(path)
            .map_err(|e| Error::Dispatch(format!("cannot bind {}: {e}", path.display())))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        eprintln!("murl-daemon: listening on {}", path.display());

        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("murl-daemon: accept failed: {e}");
                    continue;
                }
            };
            // Connections are served one at a time, so a peer that connects
            // and then says nothing would wedge the daemon for every other
            // caller. A read timeout bounds that (threat D-3); consent can
            // take a while, so the window is generous rather than tight.
            if let Err(e) = stream.set_read_timeout(Some(std::time::Duration::from_secs(300))) {
                eprintln!("murl-daemon: cannot set read timeout: {e}");
                continue;
            }
            let write_half = match stream.try_clone() {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("murl-daemon: cannot clone stream: {e}");
                    continue;
                }
            };
            // One connection at a time: dispatch is sequential anyway, and
            // serial handling is the simplest defense against flooding.
            if let Err(e) = serve_connection(ctx, stream, write_half, clock.now_epoch(), 32) {
                eprintln!("murl-daemon: connection error: {e}");
            }
        }
        Ok(())
    }
}

#[cfg(not(unix))]
pub fn run(
    _ctx: &Context<'_>,
    _path: &std::path::Path,
    _clock: &dyn murl_core::time::Clock,
) -> Result<()> {
    Err(murl_core::Error::Dispatch(
        "the daemon transport is Unix-only in v0.3; see docs/daemon.md".into(),
    ))
}
