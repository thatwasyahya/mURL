//! Routing `murl open` through the daemon when one is available.
//!
//! The contract (docs/daemon.md): the daemon is an optimization of the
//! consent surface, **never** a dependency. Absent, stale, untrusted, or
//! speaking another protocol version all mean the same thing here — fall
//! back to in-process resolution, which is fail-closed on its own.
//!
//! `--daemon` turns "fall back" into "fail", for scripts that specifically
//! want the daemon's surface; `--no-daemon` skips it entirely.

use murl_core::error::{Error, Result};
use murl_daemon::client;
use murl_daemon::protocol::{Request, Response, PROTOCOL_VERSION};
use murl_daemon::socket;

use crate::ctx::App;
use crate::logger;

/// How the CLI should treat the daemon for this invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Try the daemon; fall back silently (default).
    Auto,
    /// Require the daemon; fail if it cannot be used.
    Require,
    /// Never use the daemon.
    Never,
}

impl Mode {
    pub fn from_flags(require: bool, never: bool) -> Mode {
        match (require, never) {
            (true, _) => Mode::Require,
            (_, true) => Mode::Never,
            _ => Mode::Auto,
        }
    }
}

/// What happened.
#[derive(Debug)]
pub enum Outcome {
    /// The daemon handled the activation; this is the exit code.
    Handled(i32),
    /// Resolve in-process instead.
    FallBack,
}

pub fn try_open(app: &App, target: &str, only: &[String], mode: Mode) -> Result<Outcome> {
    if mode == Mode::Never {
        return Ok(Outcome::FallBack);
    }
    // Bundle files and bare manifest paths are a local concern; the daemon
    // only speaks mURLs.
    if !(target.len() >= 5 && target[..5].eq_ignore_ascii_case("murl:")) {
        return refuse_or_fall_back(mode, "target is a file path, not an mURL");
    }

    let path = socket::socket_path()?;
    if !client::probe(&path) {
        return refuse_or_fall_back(mode, &format!("no usable daemon at {}", path.display()));
    }

    logger::info(&format!(
        "activating through the daemon at {}",
        path.display()
    ));
    let responses = match client::request(
        &path,
        &Request::Activate {
            protocol: PROTOCOL_VERSION,
            murl: target.to_owned(),
            only: only.to_vec(),
        },
    ) {
        Ok(r) => r,
        Err(e) => return refuse_or_fall_back(mode, &e.to_string()),
    };

    let mut exit = 0;
    for response in &responses {
        match response {
            Response::Plan { resolution } => {
                if app.json {
                    println!("{}", serde_json::to_string_pretty(resolution)?);
                }
            }
            Response::Consent { granted, denied } => {
                if !app.json {
                    if !granted.is_empty() {
                        println!("granted: {}", granted.join(", "));
                    }
                    if !denied.is_empty() {
                        println!("denied:  {}", denied.join(", "));
                    }
                }
            }
            Response::Outcome { report } => {
                if app.json {
                    println!("{}", serde_json::to_string_pretty(report)?);
                } else {
                    print_report(report);
                }
                exit = exit_code_for(report);
            }
            Response::Error { stage, message } => {
                // A daemon-side error is a real error: it resolved and
                // refused, so retrying in-process would be asking a second
                // time for a "no" we already got.
                return Err(Error::Resolution(format!("daemon ({stage}): {message}")));
            }
            Response::Pong { .. } | Response::Status { .. } => {}
        }
    }
    Ok(Outcome::Handled(exit))
}

fn refuse_or_fall_back(mode: Mode, why: &str) -> Result<Outcome> {
    match mode {
        Mode::Require => Err(Error::Dispatch(format!(
            "--daemon was requested but the daemon could not be used: {why}"
        ))),
        _ => {
            logger::debug(&format!("not using the daemon: {why}"));
            Ok(Outcome::FallBack)
        }
    }
}

fn print_report(report: &serde_json::Value) {
    let aggregate = report
        .get("aggregate")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");
    println!("{aggregate}");
    if let Some(outcomes) = report.get("outcomes").and_then(|v| v.as_array()) {
        for outcome in outcomes {
            let id = outcome.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let status = outcome
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let detail = outcome
                .get("detail")
                .and_then(|v| v.as_str())
                .map(|d| format!(" ({d})"))
                .unwrap_or_default();
            println!("  {id}: {status}{detail}");
        }
    }
}

fn exit_code_for(report: &serde_json::Value) -> i32 {
    match report.get("aggregate").and_then(|v| v.as_str()) {
        Some("SUCCESS") => 0,
        Some("PARTIAL_SUCCESS") => 3,
        Some("DENIED") => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_flags() {
        assert_eq!(Mode::from_flags(false, false), Mode::Auto);
        assert_eq!(Mode::from_flags(true, false), Mode::Require);
        assert_eq!(Mode::from_flags(false, true), Mode::Never);
    }

    #[test]
    fn require_mode_refuses_instead_of_falling_back() {
        assert!(refuse_or_fall_back(Mode::Require, "nope").is_err());
        assert!(matches!(
            refuse_or_fall_back(Mode::Auto, "nope").unwrap(),
            Outcome::FallBack
        ));
    }

    #[test]
    fn exit_codes_match_the_cli_contract() {
        let report = |aggregate: &str| serde_json::json!({"aggregate": aggregate});
        assert_eq!(exit_code_for(&report("SUCCESS")), 0);
        assert_eq!(exit_code_for(&report("PARTIAL_SUCCESS")), 3);
        assert_eq!(exit_code_for(&report("DENIED")), 4);
        assert_eq!(exit_code_for(&report("FAILED")), 1);
        assert_eq!(exit_code_for(&serde_json::json!({})), 1);
    }
}
