//! Fuzz the daemon's request parser. It reads from a local socket any
//! process on the machine can connect to, so it must be total: no panic,
//! no unbounded work, and no acceptance of a request it does not fully
//! understand.

#![no_main]

use libfuzzer_sys::fuzz_target;
use murl_daemon::protocol::{parse_request, PROTOCOL_VERSION};

fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else {
        return;
    };
    // A line is one request; embedded newlines can never reach the parser.
    if line.contains('\n') {
        return;
    }
    match parse_request(line) {
        Err(response) => {
            // Every rejection must still serialize to exactly one line.
            let rendered = response.to_line();
            assert!(rendered.ends_with('\n'));
            assert_eq!(rendered.matches('\n').count(), 1, "response spans lines");
        }
        Ok(request) => {
            // Anything accepted must carry the exact protocol version;
            // there is no negotiation and no downgrade.
            assert_eq!(
                request.protocol(),
                PROTOCOL_VERSION,
                "accepted a request with a foreign protocol version"
            );
        }
    }
});
