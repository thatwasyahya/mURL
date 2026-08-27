//! Fuzz the manifest pipeline: parse → validate → canonicalize → verify.
//! None of it may panic, however hostile the bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use murl_core::manifest::Manifest;
use murl_core::trust::{signable_bytes, verify_manifest};
use murl_core::Limits;

fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();
    let Ok(manifest) = Manifest::from_slice(data, &limits) else {
        return;
    };

    // Validation walks every field; must be total.
    let _report = manifest.validate();

    // Canonicalization and signature verification must be total too —
    // they run on manifests *before* trust is established.
    let _ = signable_bytes(&manifest.raw);
    let _ = verify_manifest(&manifest.raw);
});
