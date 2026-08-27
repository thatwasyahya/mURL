//! `murl name` — manage the local name store (`murl://local/...`).

use murl_core::error::{Error, Result};
use murl_core::manifest::Manifest;
use murl_core::murl::{Murl, VersionTag};

use crate::ctx::App;
use crate::logger;

/// Accept `project-x`, `team/project-x@1.2.0`, or a full `murl://local/...`.
fn parse_local_name(name: &str) -> Result<Murl> {
    let murl = if name.len() >= 5 && name[..5].eq_ignore_ascii_case("murl:") {
        Murl::parse(name)?
    } else {
        Murl::parse(&format!("murl://local/{name}"))?
    };
    if !murl.authority.is_local() {
        return Err(Error::Validation(format!(
            "`{name}` is not a local name; the name store only holds murl://local/... entries"
        )));
    }
    if murl.selector.is_some() || murl.query.is_some() {
        return Err(Error::Validation(
            "a stored name cannot carry a selector or query".into(),
        ));
    }
    Ok(murl)
}

pub fn add(app: &App, name: &str, file: &str) -> Result<i32> {
    let murl = parse_local_name(name)?;
    let bytes =
        std::fs::read(file).map_err(|e| Error::NotFound(format!("cannot read {file}: {e}")))?;

    // Refuse to install anything that does not validate — the store must
    // only ever contain manifests the resolver will accept.
    let manifest = Manifest::from_slice(&bytes, &app.limits)?;
    let report = manifest.validate();
    for w in &report.warnings {
        logger::warn(&format!("{}: {}", w.path, w.message));
    }
    if !report.is_valid() {
        for e in &report.errors {
            eprintln!("error {}: {}", e.path, e.message);
        }
        return Err(Error::Validation(format!(
            "{file} is not a valid manifest; fix it (see `murl validate`) before installing"
        )));
    }

    // Identity binding at install time mirrors the resolver's check.
    if let Some(declared) = &manifest.doc.id {
        let declared = Murl::parse(declared)?;
        let name_matches = declared.authority == murl.authority && declared.name == murl.name;
        let version_conflicts = matches!(
            (&declared.version, &murl.version),
            (VersionTag::Pinned(a), VersionTag::Pinned(b)) if a != b
        );
        if !name_matches || version_conflicts {
            return Err(Error::Validation(format!(
                "manifest declares id `{}` but is being installed as `{}`; align them first",
                declared.identity(),
                murl.identity()
            )));
        }
    }

    let path = app.store.add(&murl, &bytes)?;
    println!("installed {} -> {}", murl.identity(), path.display());
    Ok(0)
}

pub fn list(app: &App) -> Result<i32> {
    let names = app.store.list()?;
    if app.json {
        println!("{}", serde_json::to_string_pretty(&names)?);
    } else if names.is_empty() {
        println!("(no local names; add one with `murl name add <name> <manifest.murl.json>`)");
    } else {
        for n in names {
            println!("{n}");
        }
    }
    Ok(0)
}

pub fn remove(app: &App, name: &str) -> Result<i32> {
    let murl = parse_local_name(name)?;
    if app.store.remove(&murl)? {
        println!("removed {}", murl.identity());
        Ok(0)
    } else {
        Err(Error::NotFound(format!(
            "{} is not in the local store",
            murl.identity()
        )))
    }
}
