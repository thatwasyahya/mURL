//! The daemon wire protocol: newline-delimited JSON, one request per line.
//!
//! Design constraints (docs/daemon.md, threat D-4):
//!
//! * **Closed enums.** Unknown request types are refused, never
//!   best-effort interpreted.
//! * **Exact version match.** `protocol` must equal [`PROTOCOL_VERSION`];
//!   there is no negotiation and therefore no downgrade dance.
//! * **Hard line cap.** A request line longer than [`MAX_LINE_BYTES`] is a
//!   protocol error, enforced while reading.
//! * **No dispatch without consent.** There is deliberately no request that
//!   opens resources without the daemon's own consent step.

use serde::{Deserialize, Serialize};

/// Wire protocol version. Bumped on any incompatible change.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum bytes in one request line, including the newline.
pub const MAX_LINE_BYTES: usize = 8 * 1024;

/// Client → daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Request {
    /// Liveness and version probe.
    Ping { protocol: u32 },
    /// Resolve and return the plan. Opens nothing.
    Resolve { protocol: u32, murl: String },
    /// Resolve, obtain consent through the daemon's UI, then dispatch.
    Activate {
        protocol: u32,
        murl: String,
        /// Resource ids the client suggests limiting to. A *narrowing*
        /// hint only: it can never widen what consent covers.
        #[serde(default)]
        only: Vec<String>,
    },
    /// Daemon status: uptime, activations served, socket path.
    Status { protocol: u32 },
}

impl Request {
    pub fn protocol(&self) -> u32 {
        match self {
            Request::Ping { protocol }
            | Request::Resolve { protocol, .. }
            | Request::Activate { protocol, .. }
            | Request::Status { protocol } => *protocol,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Request::Ping { .. } => "ping",
            Request::Resolve { .. } => "resolve",
            Request::Activate { .. } => "activate",
            Request::Status { .. } => "status",
        }
    }
}

/// Daemon → client. A single request may produce several responses
/// (`plan`, then `consent`, then `outcome`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Response {
    Pong {
        protocol: u32,
        version: String,
    },
    Plan {
        resolution: serde_json::Value,
    },
    Consent {
        granted: Vec<String>,
        denied: Vec<String>,
    },
    Outcome {
        report: serde_json::Value,
    },
    Status {
        version: String,
        uptime_secs: u64,
        activations: u64,
        socket: String,
    },
    Error {
        stage: String,
        message: String,
    },
}

impl Response {
    pub fn error(stage: &str, message: impl Into<String>) -> Response {
        Response::Error {
            stage: stage.to_owned(),
            message: message.into(),
        }
    }

    /// Serialize as one protocol line (JSON + `\n`).
    pub fn to_line(&self) -> String {
        // Response types are plain data; serialization cannot fail in
        // practice, and a protocol-level fallback beats a panic in a daemon.
        match serde_json::to_string(self) {
            Ok(mut s) => {
                s.push('\n');
                s
            }
            Err(e) => format!("{{\"type\":\"error\",\"stage\":\"encode\",\"message\":\"{e}\"}}\n"),
        }
    }
}

/// Parse one request line, enforcing the size cap and version match.
pub fn parse_request(line: &str) -> Result<Request, Response> {
    if line.len() > MAX_LINE_BYTES {
        return Err(Response::error(
            "protocol",
            format!("request exceeds {MAX_LINE_BYTES} bytes"),
        ));
    }
    // Same strict parser as manifests: duplicate members are refused.
    let value = murl_core::json::from_slice_strict(line.as_bytes())
        .map_err(|e| Response::error("protocol", format!("malformed request: {e}")))?;
    let request: Request = serde_json::from_value(value)
        .map_err(|e| Response::error("protocol", format!("unsupported request: {e}")))?;
    if request.protocol() != PROTOCOL_VERSION {
        return Err(Response::error(
            "protocol",
            format!(
                "protocol version {} is not supported (this daemon speaks {PROTOCOL_VERSION})",
                request.protocol()
            ),
        ));
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_requests() {
        let r = parse_request(r#"{"type":"ping","protocol":1}"#).unwrap();
        assert_eq!(r.name(), "ping");
        let r =
            parse_request(r#"{"type":"activate","protocol":1,"murl":"murl://local/x"}"#).unwrap();
        assert_eq!(r.name(), "activate");
    }

    #[test]
    fn rejects_unknown_type_and_extra_fields() {
        assert!(parse_request(r#"{"type":"launch","protocol":1}"#).is_err());
        assert!(parse_request(r#"{"type":"ping","protocol":1,"sneaky":true}"#).is_err());
    }

    #[test]
    fn rejects_version_mismatch_without_negotiating() {
        let err = parse_request(r#"{"type":"ping","protocol":2}"#).unwrap_err();
        match err {
            Response::Error { stage, message } => {
                assert_eq!(stage, "protocol");
                assert!(message.contains("not supported"), "{message}");
            }
            other => panic!("expected error, got {other:?}"),
        }
        assert!(parse_request(r#"{"type":"ping","protocol":0}"#).is_err());
    }

    #[test]
    fn rejects_oversize_and_malformed() {
        let big = format!(
            r#"{{"type":"ping","protocol":1,"pad":"{}"}}"#,
            "a".repeat(9000)
        );
        assert!(parse_request(&big).is_err());
        assert!(parse_request("not json").is_err());
        assert!(parse_request("").is_err());
        // Duplicate members are refused by the strict parser.
        assert!(parse_request(r#"{"type":"ping","protocol":1,"protocol":1}"#).is_err());
    }

    #[test]
    fn responses_are_single_lines() {
        let r = Response::Pong {
            protocol: 1,
            version: "0.3.0".into(),
        };
        let line = r.to_line();
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        // And a message containing a newline stays one line (JSON-escaped).
        let r = Response::error("resolve", "line one\nline two");
        assert_eq!(r.to_line().matches('\n').count(), 1);
    }
}
