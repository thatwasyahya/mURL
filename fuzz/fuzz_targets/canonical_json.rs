//! Fuzz MCF-1 canonicalization: for any JSON value it accepts, the output
//! must be valid JSON and canonicalization must be a fixpoint (canonicalizing
//! the canonical form yields identical bytes). Signatures depend on exactly
//! this property.

#![no_main]

use libfuzzer_sys::fuzz_target;
use murl_core::canonical::canonical_json_bytes;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return;
    };
    let Ok(bytes) = canonical_json_bytes(&value) else {
        return;
    };

    let reparsed: serde_json::Value =
        serde_json::from_slice(&bytes).expect("canonical output must be valid JSON");
    let bytes2 = canonical_json_bytes(&reparsed).expect("canonical form must re-canonicalize");
    assert_eq!(bytes, bytes2, "canonicalization is not a fixpoint");
});
