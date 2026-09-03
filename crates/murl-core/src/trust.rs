//! The trust engine: ed25519 manifest signatures, key management, the
//! pinned-key trust store, and integrity hashes.
//!
//! Trust model summary (`docs/trust-model.md`):
//!
//! * A signature proves *authorship continuity*, not goodness. Verification
//!   alone yields `SignedUnknownKey` — cryptographically valid, humanly
//!   meaningless.
//! * Trust is a local decision: the user pins a public key for an authority
//!   (`murl trust add example.com <key>`). Only then does a valid signature
//!   from that key yield `SignedTrusted`.
//! * Local manifests (local store, explicit file paths) are `Local`: the user
//!   already controls them; a signature adds nothing to their authority.
//! * There is no PKI, no key servers, no web of trust in v0.1 — pinning is
//!   simple enough to be implemented correctly and audited by reading one
//!   JSON file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::canonical::canonical_json_bytes;
use crate::error::{Error, Result};

/// Trust status of a resolved manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum TrustStatus {
    /// From the local name store or an explicit local file: user-controlled.
    Local,
    /// Fetched remotely, carrying no signature.
    UnsignedRemote,
    /// Valid signature, but the key is not pinned for this authority.
    SignedUnknownKey { key_id: String },
    /// Valid signature from a key the user pinned for this authority.
    SignedTrusted { key_id: String },
}

impl TrustStatus {
    /// Trusted enough for DANGEROUS resources under the default policy.
    pub fn is_trusted(&self) -> bool {
        matches!(self, TrustStatus::Local | TrustStatus::SignedTrusted { .. })
    }
}

impl std::fmt::Display for TrustStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustStatus::Local => f.write_str("LOCAL"),
            TrustStatus::UnsignedRemote => f.write_str("UNSIGNED"),
            TrustStatus::SignedUnknownKey { key_id } => write!(f, "SIGNED (unknown key {key_id})"),
            TrustStatus::SignedTrusted { key_id } => write!(f, "TRUSTED (key {key_id})"),
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Key id format: `ed25519:<first 16 hex chars of sha256(publicKey)>`.
pub fn key_id_for(public_key: &[u8]) -> String {
    format!("ed25519:{}", &sha256_hex(public_key)[..16])
}

/// An ed25519 signing keypair, stored as a JSON key file.
pub struct Keypair {
    signing: SigningKey,
}

impl std::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never Debug-print secret material.
        write!(f, "Keypair({})", self.key_id())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyFile {
    alg: String,
    key_id: String,
    public_key: String,
    secret_key: String,
}

impl Keypair {
    /// Requires the `keygen` feature (on by default). It is separable
    /// because generating a key is the only thing here that needs entropy,
    /// and some targets — wasm among them — have no free answer for that.
    #[cfg(feature = "keygen")]
    pub fn generate() -> Result<Keypair> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed)
            .map_err(|e| Error::Trust(format!("secure randomness unavailable: {e}")))?;
        Ok(Keypair {
            signing: SigningKey::from_bytes(&seed),
        })
    }

    pub fn public_key_b64(&self) -> String {
        B64.encode(self.signing.verifying_key().to_bytes())
    }

    pub fn key_id(&self) -> String {
        key_id_for(&self.signing.verifying_key().to_bytes())
    }

    pub fn sign(&self, bytes: &[u8]) -> [u8; 64] {
        self.signing.sign(bytes).to_bytes()
    }

    /// Write the key file. On Unix the file is created 0600 — refusing to
    /// leave private keys world-readable is not optional behavior.
    pub fn save(&self, path: &Path) -> Result<()> {
        let file = KeyFile {
            alg: "ed25519".into(),
            key_id: self.key_id(),
            public_key: self.public_key_b64(),
            secret_key: B64.encode(self.signing.to_bytes()),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&file)?;
        std::fs::write(path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Keypair> {
        let data = std::fs::read(path)
            .map_err(|e| Error::Trust(format!("cannot read key file {}: {e}", path.display())))?;
        let file: KeyFile = serde_json::from_slice(&data)
            .map_err(|e| Error::Trust(format!("malformed key file: {e}")))?;
        if file.alg != "ed25519" {
            return Err(Error::Trust(format!(
                "unsupported key algorithm `{}`",
                file.alg
            )));
        }
        let secret = B64
            .decode(&file.secret_key)
            .map_err(|e| Error::Trust(format!("bad secretKey encoding: {e}")))?;
        let seed: [u8; 32] = secret
            .try_into()
            .map_err(|_| Error::Trust("secretKey must be 32 bytes".into()))?;
        Ok(Keypair {
            signing: SigningKey::from_bytes(&seed),
        })
    }
}

/// Result of verifying a manifest's signature block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSignature {
    pub key_id: String,
    pub public_key_b64: String,
}

/// Canonical bytes a signature covers: the manifest with the `signature`
/// member removed, in MCF-1 form.
pub fn signable_bytes(raw: &Value) -> Result<Vec<u8>> {
    let mut copy = raw.clone();
    let obj = copy
        .as_object_mut()
        .ok_or_else(|| Error::Manifest("manifest must be a JSON object".into()))?;
    obj.remove("signature");
    canonical_json_bytes(&copy).map_err(|e| Error::Validation(e.to_string()))
}

/// Sign a manifest in place: strips any existing signature, signs the
/// canonical form, inserts the new signature block.
pub fn sign_manifest(raw: &mut Value, keypair: &Keypair) -> Result<()> {
    let bytes = signable_bytes(raw)?;
    let sig = keypair.sign(&bytes);
    let block = serde_json::json!({
        "alg": "ed25519",
        "keyId": keypair.key_id(),
        "publicKey": keypair.public_key_b64(),
        "sig": B64.encode(sig),
    });
    let obj = raw
        .as_object_mut()
        .ok_or_else(|| Error::Manifest("manifest must be a JSON object".into()))?;
    obj.insert("signature".into(), block);
    Ok(())
}

/// Verify a manifest's signature, if present.
///
/// * `Ok(None)` — no signature member.
/// * `Ok(Some(_))` — cryptographically valid signature (says nothing about
///   whether the key is *trusted*).
/// * `Err(_)` — signature present but malformed or invalid. This is always a
///   hard error: a manifest with a broken signature is evidence of tampering,
///   not a manifest that happens to be unsigned.
pub fn verify_manifest(raw: &Value) -> Result<Option<VerifiedSignature>> {
    let obj = raw
        .as_object()
        .ok_or_else(|| Error::Manifest("manifest must be a JSON object".into()))?;
    let Some(sig_val) = obj.get("signature") else {
        return Ok(None);
    };
    let block = sig_val
        .as_object()
        .ok_or_else(|| Error::Trust("signature must be an object".into()))?;
    let field = |name: &str| -> Result<&str> {
        block
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Trust(format!("signature.{name} missing or not a string")))
    };
    let alg = field("alg")?;
    if alg != "ed25519" {
        return Err(Error::Trust(format!(
            "unsupported signature algorithm `{alg}`"
        )));
    }
    let key_id = field("keyId")?;
    let public_key_b64 = field("publicKey")?;
    let sig_b64 = field("sig")?;

    let pk_bytes = B64
        .decode(public_key_b64)
        .map_err(|e| Error::Trust(format!("bad publicKey encoding: {e}")))?;
    let pk_arr: [u8; 32] = pk_bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::Trust("publicKey must be 32 bytes".into()))?;
    if key_id_for(&pk_arr) != key_id {
        return Err(Error::Trust("keyId does not match publicKey".into()));
    }
    let sig_bytes = B64
        .decode(sig_b64)
        .map_err(|e| Error::Trust(format!("bad sig encoding: {e}")))?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::Trust("sig must be 64 bytes".into()))?;

    let verifying = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|e| Error::Trust(format!("invalid public key: {e}")))?;
    let signature = Signature::from_bytes(&sig_arr);
    let bytes = signable_bytes(raw)?;
    verifying
        .verify(&bytes, &signature)
        .map_err(|_| Error::Trust("signature verification failed (manifest was modified after signing, or the signature is forged)".into()))?;

    Ok(Some(VerifiedSignature {
        key_id: key_id.to_owned(),
        public_key_b64: public_key_b64.to_owned(),
    }))
}

/// A key pinned for an authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedKey {
    pub key_id: String,
    pub public_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TrustDoc {
    #[serde(default)]
    authorities: BTreeMap<String, Vec<PinnedKey>>,
}

/// The pinned-key trust store, persisted as one auditable JSON file.
#[derive(Debug)]
pub struct TrustStore {
    path: PathBuf,
    doc: TrustDoc,
}

impl TrustStore {
    /// Load the store; a missing file is an empty store.
    pub fn load(path: PathBuf) -> Result<TrustStore> {
        let doc = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                Error::Trust(format!("malformed trust store {}: {e}", path.display()))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => TrustDoc::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(TrustStore { path, doc })
    }

    /// An in-memory store for tests and embedders without persistence.
    pub fn in_memory() -> TrustStore {
        TrustStore {
            path: PathBuf::new(),
            doc: TrustDoc::default(),
        }
    }

    fn save(&self) -> Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.doc)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    /// Pin a public key (base64, 32 bytes) for an authority. Returns the
    /// derived key id.
    pub fn add(&mut self, authority: &str, public_key_b64: &str, now: u64) -> Result<String> {
        let authority = normalize_authority(authority)?;
        let pk = B64
            .decode(public_key_b64)
            .map_err(|e| Error::Trust(format!("bad public key encoding: {e}")))?;
        if pk.len() != 32 {
            return Err(Error::Trust("public key must be 32 bytes".into()));
        }
        // Reject keys that are not valid curve points now, not at verify time.
        let arr: [u8; 32] = pk.as_slice().try_into().expect("length checked");
        VerifyingKey::from_bytes(&arr)
            .map_err(|e| Error::Trust(format!("invalid ed25519 public key: {e}")))?;
        let key_id = key_id_for(&pk);
        let entry = self.doc.authorities.entry(authority).or_default();
        if !entry.iter().any(|k| k.key_id == key_id) {
            entry.push(PinnedKey {
                key_id: key_id.clone(),
                public_key: public_key_b64.to_owned(),
                added_at: Some(now),
            });
        }
        self.save()?;
        Ok(key_id)
    }

    /// Remove a pinned key. Returns whether anything was removed.
    pub fn remove(&mut self, authority: &str, key_id: &str) -> Result<bool> {
        let authority = normalize_authority(authority)?;
        let mut removed = false;
        if let Some(keys) = self.doc.authorities.get_mut(&authority) {
            let before = keys.len();
            keys.retain(|k| k.key_id != key_id);
            removed = keys.len() != before;
            if keys.is_empty() {
                self.doc.authorities.remove(&authority);
            }
        }
        self.save()?;
        Ok(removed)
    }

    pub fn is_trusted(&self, authority: &str, key_id: &str) -> bool {
        let Ok(authority) = normalize_authority(authority) else {
            return false;
        };
        self.doc
            .authorities
            .get(&authority)
            .is_some_and(|keys| keys.iter().any(|k| k.key_id == key_id))
    }

    pub fn entries(&self) -> Vec<(String, PinnedKey)> {
        self.doc
            .authorities
            .iter()
            .flat_map(|(a, keys)| keys.iter().map(move |k| (a.clone(), k.clone())))
            .collect()
    }
}

fn normalize_authority(authority: &str) -> Result<String> {
    // Validate through the mURL authority parser to get one set of rules.
    let probe = format!("murl://{authority}/x");
    let m = crate::murl::Murl::parse(&probe)
        .map_err(|e| Error::Trust(format!("invalid authority `{authority}`: {e}")))?;
    Ok(m.authority.to_string())
}

/// Compute an SRI-style integrity string over raw bytes.
pub fn make_integrity(bytes: &[u8]) -> String {
    format!("sha256-{}", B64.encode(Sha256::digest(bytes)))
}

/// Check bytes against an `sha256-<base64>` integrity pin.
pub fn check_integrity(bytes: &[u8], integrity: &str) -> Result<()> {
    let Some(expected_b64) = integrity.strip_prefix("sha256-") else {
        return Err(Error::Trust(format!(
            "unsupported integrity algorithm in `{integrity}`"
        )));
    };
    let expected = B64
        .decode(expected_b64)
        .map_err(|e| Error::Trust(format!("bad integrity encoding: {e}")))?;
    let actual = Sha256::digest(bytes);
    if expected.as_slice() != actual.as_slice() {
        return Err(Error::Trust(
            "integrity mismatch: nested manifest does not match its pin".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest() -> Value {
        json!({
            "murlVersion": "0.1",
            "name": "T",
            "resources": [{"id": "a", "kind": "https", "target": "https://e.com"}]
        })
    }

    #[test]
    fn sign_verify_roundtrip() {
        let kp = Keypair::generate().unwrap();
        let mut m = manifest();
        sign_manifest(&mut m, &kp).unwrap();
        let v = verify_manifest(&m).unwrap().unwrap();
        assert_eq!(v.key_id, kp.key_id());
    }

    #[test]
    fn tamper_detection() {
        let kp = Keypair::generate().unwrap();
        let mut m = manifest();
        sign_manifest(&mut m, &kp).unwrap();
        m["name"] = json!("Tampered");
        assert!(verify_manifest(&m).is_err());
    }

    #[test]
    fn key_substitution_detected() {
        // Attacker swaps in their own key but keeps the old keyId.
        let kp1 = Keypair::generate().unwrap();
        let kp2 = Keypair::generate().unwrap();
        let mut m = manifest();
        sign_manifest(&mut m, &kp1).unwrap();
        m["signature"]["publicKey"] = json!(kp2.public_key_b64());
        assert!(verify_manifest(&m).is_err());
    }

    #[test]
    fn unsigned_is_none_not_error() {
        assert_eq!(verify_manifest(&manifest()).unwrap(), None);
    }

    #[test]
    fn resigning_replaces_signature() {
        let kp1 = Keypair::generate().unwrap();
        let kp2 = Keypair::generate().unwrap();
        let mut m = manifest();
        sign_manifest(&mut m, &kp1).unwrap();
        sign_manifest(&mut m, &kp2).unwrap();
        let v = verify_manifest(&m).unwrap().unwrap();
        assert_eq!(v.key_id, kp2.key_id());
    }

    #[test]
    fn signature_covers_unknown_members() {
        let kp = Keypair::generate().unwrap();
        let mut m = manifest();
        m["futureExtension"] = json!({"x": 1});
        sign_manifest(&mut m, &kp).unwrap();
        assert!(verify_manifest(&m).unwrap().is_some());
        m["futureExtension"]["x"] = json!(2);
        assert!(verify_manifest(&m).is_err());
    }

    #[test]
    fn trust_store_pin_and_check() {
        let kp = Keypair::generate().unwrap();
        let mut store = TrustStore::in_memory();
        let kid = store
            .add("Example.COM", &kp.public_key_b64(), 1_000)
            .unwrap();
        assert_eq!(kid, kp.key_id());
        assert!(store.is_trusted("example.com", &kid));
        assert!(!store.is_trusted("other.com", &kid));
        assert!(store.remove("example.com", &kid).unwrap());
        assert!(!store.is_trusted("example.com", &kid));
    }

    #[test]
    fn trust_store_rejects_garbage_keys() {
        let mut store = TrustStore::in_memory();
        assert!(store.add("example.com", "not-base64!!!", 0).is_err());
        assert!(store.add("example.com", &B64.encode([0u8; 16]), 0).is_err());
        assert!(store
            .add(
                "bad authority!",
                &Keypair::generate().unwrap().public_key_b64(),
                0
            )
            .is_err());
    }

    #[test]
    fn keypair_save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("murl-test-{}", std::process::id()));
        let path = dir.join("k.key.json");
        let kp = Keypair::generate().unwrap();
        kp.save(&path).unwrap();
        let loaded = Keypair::load(&path).unwrap();
        assert_eq!(loaded.key_id(), kp.key_id());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn integrity_roundtrip_and_mismatch() {
        let pin = make_integrity(b"hello");
        assert!(check_integrity(b"hello", &pin).is_ok());
        assert!(check_integrity(b"hellp", &pin).is_err());
        assert!(check_integrity(b"x", "md5-abc").is_err());
    }
}
