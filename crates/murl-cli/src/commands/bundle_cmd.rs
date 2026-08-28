//! `murl export` / `murl import` — move a destination between machines.

use std::collections::BTreeMap;
use std::path::PathBuf;

use murl_core::bundle::Bundle;
use murl_core::error::{Error, Result};
use murl_core::manifest::Manifest;
use murl_core::murl::{Authority, Murl};
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
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();
    for (i, entry) in bundle.entries.iter().enumerate() {
        // A bundle exported from a bare manifest file has no identities at
        // all (nothing named it), so its first entry is the root by
        // construction — that is the case `--as` exists to serve.
        let is_root = match &bundle.root {
            Some(root) => entry.identity.as_deref() == Some(root.as_str()),
            None => i == 0,
        };
        let local = match (is_root, as_name) {
            (true, Some(name)) => local_name(name)?,
            _ => match &entry.identity {
                // Re-home under local/ keeping the name path, but build the
                // mURL *structurally*. Re-parsing the decoded text would
                // let an escaped byte become grammar again: an identity of
                // `murl://vendor.example/tool%401` decodes to the single
                // segment `tool@1`, which re-parsed means name `tool` at
                // version 1 — a different, unrelated store slot.
                Some(identity) => {
                    let parsed = Murl::parse(identity)?;
                    Murl {
                        authority: Authority::Local,
                        name: parsed.name.clone(),
                        version: parsed.version.clone(),
                        selector: None,
                        query: None,
                    }
                }
                None => {
                    return Err(Error::Validation(format!(
                        "bundle entry {i} (`{}`) has no identity; re-export it under a name{}",
                        entry.name,
                        if is_root { ", or import with --as" } else { "" }
                    )))
                }
            },
        };

        // Identity binding, exactly as `murl name add` enforces it: a
        // manifest that declares an id the resolver would refuse must not
        // be written at all. Installing it "successfully" would leave a
        // store entry no lookup can ever read.
        let bytes = entry.decode(&app.limits)?;
        let manifest = Manifest::from_slice(&bytes, &app.limits)?;
        if let Some(declared) = &manifest.doc.id {
            let declared = Murl::parse(declared)?;
            let matches = declared.authority == local.authority && declared.name == local.name;
            if !matches {
                return Err(Error::Validation(format!(
                    "bundle entry `{}` declares id `{}`, which cannot resolve as `{}`. \
                     Re-export it from a local name, or ask the publisher for a manifest \
                     whose id matches where you can install it.",
                    entry.name,
                    declared.identity(),
                    local.identity()
                )));
            }
        }

        // Two entries that re-home onto the same local name would silently
        // overwrite each other, and the survivor would be whichever the
        // bundle author listed last.
        if let Some(previous) = claimed.insert(
            local.identity(),
            entry.identity.clone().unwrap_or_else(|| entry.name.clone()),
        ) {
            return Err(Error::Validation(format!(
                "bundle entries `{previous}` and `{}` both install as `{}`; \
                 refusing an import where one would silently overwrite the other",
                entry.identity.clone().unwrap_or_else(|| entry.name.clone()),
                local.identity()
            )));
        }

        plan.push((local, bytes));
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
