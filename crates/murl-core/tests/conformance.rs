//! Runs the shared conformance suite in `spec/conformance/` against this
//! implementation.
//!
//! The suite is the contract an independent implementation can test itself
//! against; here it doubles as a regression net. Each test asserts a minimum
//! vector count first, so a wrong path fails loudly instead of passing
//! vacuously over an empty directory.

use std::path::{Path, PathBuf};

use murl_core::manifest::Manifest;
use murl_core::murl::Murl;
use murl_core::Limits;

fn suite_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/conformance")
}

fn read_manifests(sub: &str) -> Vec<(String, Vec<u8>)> {
    let dir = suite_dir().join("manifests").join(sub);
    let mut out = Vec::new();
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".murl.json") {
            out.push((name, std::fs::read(entry.path()).unwrap()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn read_lines(name: &str) -> Vec<String> {
    let path = suite_dir().join("murls").join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn valid_manifests_parse_and_validate() {
    let vectors = read_manifests("valid");
    assert!(
        vectors.len() >= 10,
        "expected >=10 valid vectors, found {} — is the suite path right?",
        vectors.len()
    );
    for (name, bytes) in vectors {
        let manifest = Manifest::from_slice(&bytes, &Limits::default())
            .unwrap_or_else(|e| panic!("valid vector `{name}` failed to parse: {e}"));
        let report = manifest.validate();
        assert!(
            report.is_valid(),
            "valid vector `{name}` failed validation: {:?}",
            report.errors
        );
    }
}

#[test]
fn invalid_manifests_are_rejected() {
    let vectors = read_manifests("invalid");
    assert!(
        vectors.len() >= 18,
        "expected >=18 invalid vectors, found {}",
        vectors.len()
    );
    for (name, bytes) in vectors {
        match Manifest::from_slice(&bytes, &Limits::default()) {
            Err(_) => {} // rejected at parse time (e.g. duplicate members)
            Ok(manifest) => {
                let report = manifest.validate();
                assert!(
                    !report.is_valid(),
                    "invalid vector `{name}` was accepted with no errors"
                );
            }
        }
    }
}

#[test]
fn valid_murls_parse_and_round_trip() {
    let lines = read_lines("valid.txt");
    assert!(
        lines.len() >= 15,
        "expected >=15 valid mURLs, found {}",
        lines.len()
    );
    for line in lines {
        let parsed = Murl::parse(&line)
            .unwrap_or_else(|e| panic!("valid mURL `{line}` failed to parse: {e}"));
        let canonical = parsed.to_string();
        let reparsed = Murl::parse(&canonical).unwrap_or_else(|e| {
            panic!("canonical form `{canonical}` of `{line}` failed to reparse: {e}")
        });
        assert_eq!(parsed, reparsed, "`{line}` did not round-trip");
    }
}

#[test]
fn invalid_murls_are_rejected() {
    let lines = read_lines("invalid.txt");
    assert!(
        lines.len() >= 20,
        "expected >=20 invalid mURLs, found {}",
        lines.len()
    );
    for line in lines {
        assert!(
            Murl::parse(&line).is_err(),
            "invalid mURL `{line}` was accepted"
        );
    }
}
