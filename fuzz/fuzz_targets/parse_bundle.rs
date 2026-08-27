//! Fuzz the bundle parser: `murl import` feeds it attacker-supplied files,
//! and it decodes base64, checks hashes, and validates carried manifests.
//! None of that may panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use murl_core::bundle::Bundle;
use murl_core::Limits;

fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();
    let Ok(bundle) = Bundle::from_slice(data, &limits) else {
        return;
    };

    // A bundle that parsed must survive being re-encoded and re-read, and
    // every entry must decode without panicking.
    for entry in &bundle.entries {
        let _ = entry.decode(&limits);
    }
    let bytes = bundle.to_json_bytes().expect("re-encoding a parsed bundle");
    let reparsed =
        Bundle::from_slice(&bytes, &limits).expect("a bundle we just serialized must parse again");
    assert_eq!(
        reparsed.entries.len(),
        bundle.entries.len(),
        "round-trip changed the entry count"
    );
});
