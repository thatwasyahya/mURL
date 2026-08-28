//! MCF-1 canonical-form vectors from `spec/conformance/canonical/`.
//!
//! These exist because a second implementation passed all 137 manifest and
//! identifier vectors while its canonical form was never tested — the suite
//! covered the grammar and the schema and skipped the one artifact that has
//! to agree byte-for-byte for signatures to interoperate. A canonical form
//! that is only checked against itself is not checked at all.

use std::path::{Path, PathBuf};

use murl_core::canonical::canonical_json_bytes;

fn vector_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/conformance/canonical")
}

#[test]
fn canonical_vectors_match_byte_for_byte() {
    let dir = vector_dir();
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));

    let mut checked = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".input.json") else {
            continue;
        };

        let input = std::fs::read(&path).unwrap();
        let expected = std::fs::read(dir.join(format!("{stem}.expected")))
            .unwrap_or_else(|e| panic!("vector `{stem}` has no .expected file: {e}"));

        // The input is parsed with the strict manifest parser, because that
        // is what a signer actually feeds the canonicalizer.
        let value: serde_json::Value = murl_core::json::from_slice_strict(&input)
            .unwrap_or_else(|e| panic!("vector `{stem}` input is not valid: {e}"));
        let actual = canonical_json_bytes(&value)
            .unwrap_or_else(|e| panic!("vector `{stem}` failed to canonicalize: {e}"));

        assert_eq!(
            String::from_utf8_lossy(&actual),
            String::from_utf8_lossy(&expected),
            "vector `{stem}` does not match its expected canonical form"
        );
        checked += 1;
    }

    assert!(
        checked >= 12,
        "expected >=12 canonical vectors, found {checked} — is the suite path right?"
    );
}

/// Canonicalizing an already-canonical document must be a no-op. If it is
/// not, signing is not idempotent and a re-signed manifest changes bytes
/// for no reason.
#[test]
fn canonicalization_is_a_fixpoint_on_every_vector() {
    let dir = vector_dir();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".expected") {
            continue;
        }
        let expected = std::fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&expected)
            .unwrap_or_else(|e| panic!("`{name}` is not valid JSON: {e}"));
        let again = canonical_json_bytes(&value).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&again),
            String::from_utf8_lossy(&expected),
            "`{name}` is not a fixpoint"
        );
    }
}
