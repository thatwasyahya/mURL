//! Fuzz the mURL parser: it must never panic on any input, and every
//! accepted mURL must round-trip through its canonical display form.

#![no_main]

use libfuzzer_sys::fuzz_target;
use murl_core::Murl;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(parsed) = Murl::parse(s) else { return };

    // Canonical round-trip: display must reparse to the same value.
    let canonical = parsed.to_string();
    let reparsed = Murl::parse(&canonical)
        .unwrap_or_else(|e| panic!("canonical form `{canonical}` failed to reparse: {e}"));
    assert_eq!(parsed, reparsed, "round-trip changed the parse of `{s}`");

    // The identity must itself be a valid mURL and idempotent.
    let identity = parsed.identity();
    let id_parsed = Murl::parse(&identity)
        .unwrap_or_else(|e| panic!("identity `{identity}` failed to parse: {e}"));
    assert_eq!(id_parsed.identity(), identity, "identity is not a fixpoint");
});
