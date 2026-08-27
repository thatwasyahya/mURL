//! Bundles: one file carrying a destination and everything it composes.
//!
//! A bundle is how an mURL travels somewhere its authority cannot be reached
//! — an air-gapped machine, an archive, a bug report, a review attachment.
//! It carries the root manifest plus every nested manifest reached during
//! resolution, each as **verbatim bytes**, because a signed manifest that is
//! re-serialized is a signed manifest no longer.
//!
//! Security posture: importing a bundle is exactly as trusted as importing
//! the manifests individually. Bundles carry no permissions, no trust
//! assertions, and no keys; a signature inside one still verifies against
//! the local trust store (or does not). `murl import` installs into the
//! *local* namespace under names the user chooses, so an imported bundle can
//! never claim `murl://someone-else.example/...`.

use std::collections::BTreeMap;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::manifest::Manifest;
use crate::murl::Murl;
use crate::resolver::Resolution;
use crate::trust::{make_integrity, sha256_hex};

/// The bundle format version this implementation reads and writes.
pub const BUNDLE_VERSION: &str = "0.2";

/// Maximum number of manifests a bundle may carry.
pub const MAX_BUNDLE_ENTRIES: usize = 64;

/// One manifest inside a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleEntry {
    /// Canonical mURL identity this manifest was resolved as, when it had
    /// one (a bundle exported from a bare file has none for its root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// Human name, for listing a bundle without decoding it.
    pub name: String,
    /// `sha256-<base64>` over `bytes`, checked on import.
    pub integrity: String,
    /// The manifest, base64 of its exact original bytes.
    pub bytes: String,
}

/// A portable set of manifests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub bundle_version: String,
    /// The identity of the root destination, when it had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// Root first, then nested manifests in resolution order.
    pub entries: Vec<BundleEntry>,
}

impl Bundle {
    /// Build a bundle from a completed resolution.
    pub fn from_resolution(resolution: &Resolution) -> Result<Bundle> {
        if resolution.nodes.len() > MAX_BUNDLE_ENTRIES {
            return Err(Error::LimitExceeded(format!(
                "resolution has {} manifests, bundle limit is {MAX_BUNDLE_ENTRIES}",
                resolution.nodes.len()
            )));
        }
        let entries = resolution
            .nodes
            .iter()
            .map(|node| BundleEntry {
                identity: node.identity.clone(),
                name: node.manifest.doc.name.clone(),
                integrity: make_integrity(&node.raw_bytes),
                bytes: B64.encode(&node.raw_bytes),
            })
            .collect();
        Ok(Bundle {
            bundle_version: BUNDLE_VERSION.to_owned(),
            root: resolution.root().identity.clone(),
            entries,
        })
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Parse and structurally verify a bundle: version, entry count,
    /// per-entry integrity, and that every manifest still parses and
    /// validates. Nothing is installed and no trust is conferred.
    pub fn from_slice(bytes: &[u8], limits: &Limits) -> Result<Bundle> {
        // Bundles hold up to 64 manifests, so they are legitimately larger
        // than one manifest — but still bounded.
        let cap = limits
            .max_manifest_bytes
            .saturating_mul(MAX_BUNDLE_ENTRIES.min(8));
        if bytes.len() > cap {
            return Err(Error::LimitExceeded(format!(
                "bundle is {} bytes, limit is {cap}",
                bytes.len()
            )));
        }
        let value: Value = crate::json::from_slice_strict(bytes)
            .map_err(|e| Error::Manifest(format!("invalid bundle JSON: {e}")))?;
        let bundle: Bundle = serde_json::from_value(value)
            .map_err(|e| Error::Manifest(format!("bundle schema mismatch: {e}")))?;

        if bundle.bundle_version != BUNDLE_VERSION {
            return Err(Error::Validation(format!(
                "unsupported bundle version `{}` (expected `{BUNDLE_VERSION}`)",
                bundle.bundle_version
            )));
        }
        if bundle.entries.is_empty() {
            return Err(Error::Validation("bundle carries no manifests".into()));
        }
        if bundle.entries.len() > MAX_BUNDLE_ENTRIES {
            return Err(Error::LimitExceeded(format!(
                "bundle carries {} manifests, limit is {MAX_BUNDLE_ENTRIES}",
                bundle.entries.len()
            )));
        }

        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        for (i, entry) in bundle.entries.iter().enumerate() {
            let raw = entry.decode(limits)?;
            crate::trust::check_integrity(&raw, &entry.integrity)
                .map_err(|e| Error::Trust(format!("bundle entry {i} (`{}`): {e}", entry.name)))?;
            // Every carried manifest must itself be valid — a bundle is not
            // a smuggling route around validation.
            let manifest = Manifest::from_slice(&raw, limits)?;
            manifest.validate().into_result()?;
            if let Some(identity) = &entry.identity {
                let parsed = Murl::parse(identity)?;
                if parsed.identity() != *identity {
                    return Err(Error::Validation(format!(
                        "bundle entry {i} identity `{identity}` is not canonical"
                    )));
                }
                if seen.insert(identity.clone(), ()).is_some() {
                    return Err(Error::Validation(format!(
                        "bundle contains duplicate identity `{identity}`"
                    )));
                }
            }
        }
        if let Some(root) = &bundle.root {
            if !bundle
                .entries
                .iter()
                .any(|e| e.identity.as_deref() == Some(root.as_str()))
            {
                return Err(Error::Validation(format!(
                    "bundle root `{root}` is not among its entries"
                )));
            }
        }
        Ok(bundle)
    }

    /// A short digest of the bundle bytes, for logs and provenance.
    pub fn fingerprint(bytes: &[u8]) -> String {
        sha256_hex(bytes)[..16].to_owned()
    }
}

impl BundleEntry {
    /// Decode this entry's manifest bytes, enforcing the per-manifest cap.
    pub fn decode(&self, limits: &Limits) -> Result<Vec<u8>> {
        let raw = B64.decode(&self.bytes).map_err(|e| {
            Error::Manifest(format!("bundle entry `{}`: bad base64: {e}", self.name))
        })?;
        if raw.len() > limits.max_manifest_bytes {
            return Err(Error::LimitExceeded(format!(
                "bundle entry `{}` is {} bytes, limit is {}",
                self.name,
                raw.len(),
                limits.max_manifest_bytes
            )));
        }
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry_bytes() -> Vec<u8> {
        serde_json::to_vec(&json!({
            "murlVersion": "0.2",
            "name": "T",
            "resources": [{"id": "a", "kind": "https", "target": "https://e.com"}]
        }))
        .unwrap()
    }

    fn bundle_with(entry: BundleEntry, root: Option<&str>) -> Vec<u8> {
        let b = Bundle {
            bundle_version: BUNDLE_VERSION.into(),
            root: root.map(str::to_owned),
            entries: vec![entry],
        };
        b.to_json_bytes().unwrap()
    }

    fn good_entry() -> BundleEntry {
        let raw = entry_bytes();
        BundleEntry {
            identity: Some("murl://local/t".into()),
            name: "T".into(),
            integrity: make_integrity(&raw),
            bytes: B64.encode(&raw),
        }
    }

    #[test]
    fn roundtrip() {
        let bytes = bundle_with(good_entry(), Some("murl://local/t"));
        let parsed = Bundle::from_slice(&bytes, &Limits::default()).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.root.as_deref(), Some("murl://local/t"));
        assert_eq!(
            parsed.entries[0].decode(&Limits::default()).unwrap(),
            entry_bytes()
        );
    }

    #[test]
    fn rejects_integrity_mismatch() {
        let mut e = good_entry();
        e.integrity = make_integrity(b"different");
        let bytes = bundle_with(e, None);
        let err = Bundle::from_slice(&bytes, &Limits::default()).unwrap_err();
        assert!(matches!(err, Error::Trust(_)), "{err}");
    }

    #[test]
    fn rejects_invalid_carried_manifest() {
        let raw = br#"{"murlVersion":"0.2","name":"B","resources":[]}"#.to_vec();
        let e = BundleEntry {
            identity: None,
            name: "B".into(),
            integrity: make_integrity(&raw),
            bytes: B64.encode(&raw),
        };
        let bytes = bundle_with(e, None);
        assert!(Bundle::from_slice(&bytes, &Limits::default()).is_err());
    }

    #[test]
    fn rejects_wrong_version_and_empty() {
        let bytes = serde_json::to_vec(&json!({
            "bundleVersion": "9.9", "entries": [] , "root": null
        }))
        .unwrap();
        assert!(Bundle::from_slice(&bytes, &Limits::default()).is_err());
        let bytes = serde_json::to_vec(&json!({
            "bundleVersion": BUNDLE_VERSION, "entries": []
        }))
        .unwrap();
        assert!(Bundle::from_slice(&bytes, &Limits::default()).is_err());
    }

    #[test]
    fn rejects_dangling_root_and_duplicate_identities() {
        let bytes = bundle_with(good_entry(), Some("murl://local/other"));
        assert!(Bundle::from_slice(&bytes, &Limits::default()).is_err());

        let b = Bundle {
            bundle_version: BUNDLE_VERSION.into(),
            root: None,
            entries: vec![good_entry(), good_entry()],
        };
        let bytes = b.to_json_bytes().unwrap();
        assert!(Bundle::from_slice(&bytes, &Limits::default()).is_err());
    }

    #[test]
    fn rejects_duplicate_json_members() {
        let raw = br#"{"bundleVersion":"0.2","bundleVersion":"0.2","entries":[]}"#;
        let err = Bundle::from_slice(raw, &Limits::default()).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err}");
    }
}
