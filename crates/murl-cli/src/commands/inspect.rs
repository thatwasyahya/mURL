//! `murl inspect` — human-oriented summary of one manifest: metadata,
//! resources with tiers, trust status, validation state.

use murl_core::kind::Kind;
use murl_core::policy::classify;
use murl_core::trust::verify_manifest;
use murl_core::Result;
use serde_json::json;

use crate::ctx::App;

pub fn run(app: &App, target: &str) -> Result<i32> {
    let (manifest, origin, _warnings, murl) = app.fetch_root_of(target)?;
    let report = manifest.validate();

    let trust_line = match verify_manifest(&manifest.raw) {
        Err(e) => format!("INVALID SIGNATURE — {e}"),
        Ok(None) => {
            if origin.is_remote() {
                "UNSIGNED (remote)".to_string()
            } else {
                "LOCAL (unsigned)".to_string()
            }
        }
        Ok(Some(v)) => {
            if origin.is_remote() {
                let authority = murl
                    .as_ref()
                    .map(|m| m.authority.to_string())
                    .unwrap_or_default();
                if app.trust.borrow().is_trusted(&authority, &v.key_id) {
                    format!("TRUSTED — signed with pinned key {}", v.key_id)
                } else {
                    format!("SIGNED — key {} is not pinned for `{authority}`", v.key_id)
                }
            } else {
                format!("LOCAL (signed, key {})", v.key_id)
            }
        }
    };

    if app.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "target": target,
                "origin": origin.describe(),
                "trust": trust_line,
                "valid": report.is_valid(),
                "errors": report.errors,
                "warnings": report.warnings,
                "manifest": manifest.raw,
            }))?
        );
        return Ok(0);
    }

    let d = &manifest.doc;
    println!("name:        {}", d.name);
    if let Some(id) = &d.id {
        println!("id:          {id}");
    }
    if let Some(desc) = &d.description {
        println!("description: {desc}");
    }
    if let Some(v) = &d.version {
        println!("version:     {v}");
    }
    if let Some(exp) = &d.expires {
        println!("expires:     {exp}");
    }
    println!("origin:      {}", origin.describe());
    println!("trust:       {trust_line}");
    println!(
        "validation:  {}",
        if report.is_valid() {
            format!("valid ({} warnings)", report.warnings.len())
        } else {
            format!("{} ERRORS", report.errors.len())
        }
    );

    println!("\nresources ({}):", d.resources.len());
    let id_width = d.resources.iter().map(|r| r.id.len()).max().unwrap_or(0);
    for r in &d.resources {
        let tier = Kind::parse(&r.kind)
            .map(|k| classify(&k, &r.target).to_string())
            .unwrap_or_else(|_| "?".into());
        println!(
            "  {:id_width$}  {:9}  {:9}  {}{}",
            r.id,
            tier,
            r.kind,
            r.target,
            r.role
                .as_ref()
                .map(|role| format!("  [{role}]"))
                .unwrap_or_default()
        );
    }
    if !d.relations.is_empty() {
        println!("\nrelations:");
        for rel in &d.relations {
            println!("  {} --{}--> {}", rel.from, rel.rel, rel.to);
        }
    }
    Ok(0)
}
