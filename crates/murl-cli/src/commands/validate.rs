//! `murl validate` — validate a manifest against the specification.
//!
//! Exit codes: 0 valid, 2 invalid.

use murl_core::Result;
use serde_json::json;

use crate::ctx::App;
use crate::logger;

pub fn run(app: &App, target: &str) -> Result<i32> {
    let (manifest, origin, res_warnings, _murl) = app.fetch_root_of(target)?;
    for w in &res_warnings {
        logger::warn(w);
    }
    let report = manifest.validate();

    if app.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "target": target,
                "origin": origin.describe(),
                "valid": report.is_valid(),
                "errors": report.errors,
                "warnings": report.warnings,
            }))?
        );
    } else {
        for issue in &report.errors {
            println!("error   {}: {}", issue.path, issue.message);
        }
        for issue in &report.warnings {
            println!("warning {}: {}", issue.path, issue.message);
        }
        if report.is_valid() {
            println!(
                "OK: `{}` is a valid mURL v0.1 manifest ({} warning{})",
                target,
                report.warnings.len(),
                if report.warnings.len() == 1 { "" } else { "s" }
            );
        } else {
            println!(
                "INVALID: {} error{}, {} warning{}",
                report.errors.len(),
                if report.errors.len() == 1 { "" } else { "s" },
                report.warnings.len(),
                if report.warnings.len() == 1 { "" } else { "s" }
            );
        }
    }
    Ok(if report.is_valid() { 0 } else { 2 })
}
