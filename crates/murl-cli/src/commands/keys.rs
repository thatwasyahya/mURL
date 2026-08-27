//! `murl keygen` / `murl sign` / `murl verify` — signing workflow.

use std::path::PathBuf;

use murl_core::error::{Error, Result};
use murl_core::manifest::Manifest;
use murl_core::trust::{sign_manifest, verify_manifest, Keypair};
use serde_json::json;

use crate::ctx::App;

pub fn keygen(app: &App, out: Option<&str>, force: bool) -> Result<i32> {
    let path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| app.paths.default_key_file());
    if path.exists() && !force {
        return Err(Error::Trust(format!(
            "{} already exists; refusing to overwrite a signing key (pass --force if you mean it)",
            path.display()
        )));
    }
    let kp = Keypair::generate()?;
    kp.save(&path)?;
    if app.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "keyId": kp.key_id(),
                "publicKey": kp.public_key_b64(),
                "path": path.display().to_string(),
            }))?
        );
    } else {
        println!("generated ed25519 keypair");
        println!("  keyId:     {}", kp.key_id());
        println!("  publicKey: {}", kp.public_key_b64());
        println!(
            "  file:      {} (private — never share this file)",
            path.display()
        );
        println!();
        println!("publish the public key so consumers can pin it:");
        println!("  murl trust add <your-authority> {}", kp.public_key_b64());
    }
    Ok(0)
}

pub fn sign(app: &App, file: &str, key: Option<&str>) -> Result<i32> {
    let key_path = key
        .map(PathBuf::from)
        .unwrap_or_else(|| app.paths.default_key_file());
    let kp = Keypair::load(&key_path)?;

    let bytes =
        std::fs::read(file).map_err(|e| Error::NotFound(format!("cannot read {file}: {e}")))?;
    // Refuse to sign an invalid manifest: a signature is a statement, and
    // "I vouch for this malformed thing" is not one worth making.
    let manifest = Manifest::from_slice(&bytes, &app.limits)?;
    let report = manifest.validate();
    if !report.is_valid() {
        for e in &report.errors {
            eprintln!("error {}: {}", e.path, e.message);
        }
        return Err(Error::Validation(format!(
            "{file} is invalid; fix it before signing"
        )));
    }

    let mut value = manifest.raw;
    sign_manifest(&mut value, &kp)?;
    let mut out = serde_json::to_vec_pretty(&value)?;
    out.push(b'\n');
    std::fs::write(file, out)?;
    println!("signed {file} with key {}", kp.key_id());
    Ok(0)
}

/// Exit codes: 0 valid signature, 2 unsigned, 1 invalid.
pub fn verify(app: &App, target: &str) -> Result<i32> {
    let (manifest, origin, _warnings, murl) = app.fetch_root_of(target)?;
    match verify_manifest(&manifest.raw) {
        Err(e) => {
            println!("INVALID: {e}");
            Ok(1)
        }
        Ok(None) => {
            println!("UNSIGNED: `{target}` carries no signature");
            Ok(2)
        }
        Ok(Some(v)) => {
            println!("VALID: signed with key {}", v.key_id);
            if origin.is_remote() {
                let authority = murl
                    .as_ref()
                    .map(|m| m.authority.to_string())
                    .unwrap_or_default();
                if app.trust.borrow().is_trusted(&authority, &v.key_id) {
                    println!("TRUSTED: key is pinned for `{authority}`");
                } else {
                    println!(
                        "NOT PINNED: pin it with `murl trust add {authority} {}`",
                        v.public_key_b64
                    );
                }
            }
            Ok(0)
        }
    }
}
