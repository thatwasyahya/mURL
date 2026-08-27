//! `murl trust` — pin, list, and remove signing keys per authority.

use murl_core::error::{Error, Result};
use murl_core::time::Clock;
use serde_json::{json, Value};

use crate::ctx::App;

/// The key argument may be a raw base64 public key, or a path to a JSON file
/// carrying one: a key file (`publicKey` member) or a signed manifest
/// (`signature.publicKey`).
fn extract_key(key: &str) -> Result<String> {
    let path = std::path::Path::new(key);
    if !path.exists() {
        return Ok(key.to_owned());
    }
    let bytes = std::fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Trust(format!("{key} is not JSON: {e}")))?;
    value
        .get("publicKey")
        .or_else(|| value.get("signature").and_then(|s| s.get("publicKey")))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            Error::Trust(format!(
                "{key} contains no `publicKey` (expected a key file or a signed manifest)"
            ))
        })
}

pub fn add(app: &App, authority: &str, key: &str) -> Result<i32> {
    let public_key = extract_key(key)?;
    let now = app.clock.now_epoch();
    let key_id = app.trust.borrow_mut().add(authority, &public_key, now)?;
    println!("pinned key {key_id} for `{authority}`");
    println!("signed manifests from this authority with this key are now TRUSTED");
    Ok(0)
}

pub fn list(app: &App) -> Result<i32> {
    let entries = app.trust.borrow().entries();
    if app.json {
        let items: Vec<Value> = entries
            .iter()
            .map(|(authority, k)| {
                json!({
                    "authority": authority,
                    "keyId": k.key_id,
                    "publicKey": k.public_key,
                    "addedAt": k.added_at,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if entries.is_empty() {
        println!("(no pinned keys; pin one with `murl trust add <authority> <publicKey>`)");
    } else {
        for (authority, k) in entries {
            println!("{authority}  {}  {}", k.key_id, k.public_key);
        }
    }
    Ok(0)
}

pub fn remove(app: &App, authority: &str, key_id: &str) -> Result<i32> {
    if app.trust.borrow_mut().remove(authority, key_id)? {
        println!("removed {key_id} for `{authority}`");
        Ok(0)
    } else {
        Err(Error::NotFound(format!(
            "no pinned key {key_id} for `{authority}`"
        )))
    }
}
