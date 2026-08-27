//! `murl export` / `murl import` — move a destination between machines.

use std::path::PathBuf;

use murl_core::bundle::Bundle;
use murl_core::error::{Error, Result};
use murl_core::murl::Murl;
use serde_json::json;

use crate::ctx::App;
use crate::logger;

/// `murl export <target> [-o file]` — resolve a destination and write it,
/// with every nested manifest, as one portable bundle.
pub fn export(app: &App, target: &str, output: Option<&str>) -> Result<i32> {
    let resolution = app.resolve_target(target)?;
    for w in &resolution.warnings {
        logger::warn(w);
    }
    let bundle = Bundle::from_resolution(&resolution)?;
    let bytes = bundle.to_json_bytes()?;
    let fingerprint = Bundle::fingerprint(&bytes);

    match output {
        Some("-") => {
            print!("{}", String::from_utf8_lossy(&bytes));
        }
        other => {
            let path = other.map(PathBuf::from).unwrap_or_else(|| {
                let stem = resolution
                    .root()
                    .identity
                    .as_deref()
                    .and_then(|id| id.rsplit('/').next())
                    .unwrap_or("destination")
                    .replace(['@', '%'], "-");
                PathBuf::from(format!("{stem}.murlbundle.json"))
            });
            std::fs::write(&path, &bytes)?;
            if app.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "path": path.display().to_string(),
                        "manifests": bundle.entries.len(),
                        "root": bundle.root,
                        "fingerprint": fingerprint,
                    }))?
                );
            } else {
                println!(
                    "exported {} manifest{} to {} (fingerprint {fingerprint})",
                    bundle.entries.len(),
                    if bundle.entries.len() == 1 { "" } else { "s" },
                    path.display()
                );
            }
        }
    }
    Ok(0)
}

/// `murl import <file> [--as <name>]` — verify a bundle and install its
/// manifests into the **local** namespace.
///
/// Imported manifests always land under `murl://local/...`: a bundle can
/// describe what it was, but it can never claim someone else's namespace on
/// this machine. Existing names are never silently overwritten.
pub fn import(app: &App, file: &str, as_name: Option<&str>, force: bool) -> Result<i32> {
    let bytes =
        std::fs::read(file).map_err(|e| Error::NotFound(format!("cannot read {file}: {e}")))?;
    let fingerprint = Bundle::fingerprint(&bytes);
    let bundle = Bundle::from_slice(&bytes, &app.limits)?;

    // Decide the local name for each entry before touching the store, so a
    // rejected entry aborts the whole import.
    let mut plan: Vec<(Murl, Vec<u8>)> = Vec::with_capacity(bundle.entries.len());
    for (i, entry) in bundle.entries.iter().enumerate() {
        let is_root = bundle.root.is_some() && entry.identity == bundle.root;
        let local = match (is_root, as_name) {
            (true, Some(name)) => local_name(name)?,
            _ => match &entry.identity {
                Some(identity) => {
                    let parsed = Murl::parse(identity)?;
                    // Remote identities are re-homed under local/, keeping
                    // their name path so composition still resolves.
                    local_name(&parsed.name_path())?
                }
                None => {
                    return Err(Error::Validation(format!(
                        "bundle entry {i} (`{}`) has no identity; re-export it under a name, or import with --as",
                        entry.name
                    )))
                }
            },
        };
        plan.push((local, entry.decode(&app.limits)?));
    }

    if !force {
        for (murl, _) in &plan {
            if app.store.manifest_path(murl).exists() {
                return Err(Error::Validation(format!(
                    "{} already exists in the local store (pass --force to overwrite)",
                    murl.identity()
                )));
            }
        }
    }

    let mut installed = Vec::with_capacity(plan.len());
    for (murl, raw) in &plan {
        app.store.add(murl, raw)?;
        installed.push(murl.identity());
    }

    if app.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "fingerprint": fingerprint,
                "installed": installed,
            }))?
        );
    } else {
        println!("verified bundle {fingerprint}");
        for identity in &installed {
            println!("  installed {identity}");
        }
        println!(
            "\nnote: imported manifests live in your local namespace; nested references\n\
             that pointed at remote authorities still resolve remotely."
        );
    }
    Ok(0)
}

fn local_name(name: &str) -> Result<Murl> {
    let murl = if name.len() >= 5 && name[..5].eq_ignore_ascii_case("murl:") {
        Murl::parse(name)?
    } else {
        Murl::parse(&format!("murl://local/{name}"))?
    };
    if !murl.authority.is_local() {
        return Err(Error::Validation(format!(
            "`{name}` is not a local name; imports always install under murl://local/..."
        )));
    }
    if murl.selector.is_some() || murl.query.is_some() {
        return Err(Error::Validation(
            "an installed name cannot carry a selector or query".into(),
        ));
    }
    Ok(murl)
}
