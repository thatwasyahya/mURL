//! The client side: talk to a running daemon, or say cleanly that there
//! isn't one.
//!
//! Every failure mode here — no socket, wrong owner, wrong protocol,
//! truncated reply — resolves to "no usable daemon", so the caller falls
//! back to in-process resolution instead of trusting an unknown listener
//! (threat D-5).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use crate::protocol::{Request, Response, MAX_LINE_BYTES, PROTOCOL_VERSION};
use crate::socket;

/// Why a daemon could not be used. Never fatal on its own.
#[derive(Debug)]
pub enum Unavailable {
    NoSocket,
    Untrusted(String),
    Io(std::io::Error),
    Protocol(String),
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unavailable::NoSocket => f.write_str("no daemon socket"),
            Unavailable::Untrusted(why) => write!(f, "socket not trusted: {why}"),
            Unavailable::Io(e) => write!(f, "i/o: {e}"),
            Unavailable::Protocol(m) => write!(f, "protocol: {m}"),
        }
    }
}

/// Send one request and collect every response line it produces.
#[cfg(unix)]
pub fn request(path: &Path, request: &Request) -> std::result::Result<Vec<Response>, Unavailable> {
    use std::os::unix::net::UnixStream;

    if !path.exists() {
        return Err(Unavailable::NoSocket);
    }
    if !socket::client_may_trust(path) {
        return Err(Unavailable::Untrusted(format!(
            "{} is not a user-private socket",
            path.display()
        )));
    }
    let stream = UnixStream::connect(path).map_err(Unavailable::Io)?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(120)))
        .map_err(Unavailable::Io)?;

    let mut line =
        serde_json::to_string(request).map_err(|e| Unavailable::Protocol(e.to_string()))?;
    if line.len() > MAX_LINE_BYTES {
        return Err(Unavailable::Protocol("request too large".into()));
    }
    line.push('\n');

    let mut write_half = stream.try_clone().map_err(Unavailable::Io)?;
    write_half
        .write_all(line.as_bytes())
        .map_err(Unavailable::Io)?;
    write_half.flush().map_err(Unavailable::Io)?;
    // Signal end-of-request so the daemon can stop reading.
    let _ = write_half.shutdown(std::net::Shutdown::Write);

    let mut responses = Vec::new();
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line.map_err(Unavailable::Io)?;
        if line.trim().is_empty() {
            continue;
        }
        let response: Response = serde_json::from_str(&line)
            .map_err(|e| Unavailable::Protocol(format!("bad response: {e}")))?;
        responses.push(response);
    }
    if responses.is_empty() {
        return Err(Unavailable::Protocol(
            "daemon closed without replying".into(),
        ));
    }
    Ok(responses)
}

#[cfg(not(unix))]
pub fn request(
    _path: &Path,
    _request: &Request,
) -> std::result::Result<Vec<Response>, Unavailable> {
    Err(Unavailable::NoSocket)
}

/// Is a usable daemon listening? Answers by pinging and checking the
/// protocol version it reports.
pub fn probe(path: &Path) -> bool {
    match request(
        path,
        &Request::Ping {
            protocol: PROTOCOL_VERSION,
        },
    ) {
        Ok(responses) => responses
            .iter()
            .any(|r| matches!(r, Response::Pong { protocol, .. } if *protocol == PROTOCOL_VERSION)),
        Err(_) => false,
    }
}
