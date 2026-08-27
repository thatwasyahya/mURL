//! `murl open` — resolve, obtain consent, dispatch, report.
//!
//! Exit codes: 0 success, 1 failed, 3 partial success, 4 denied.

use murl_core::dispatch::{execute, AggregateStatus};
use murl_core::Result;
use serde_json::json;

use crate::consent;
use crate::ctx::App;
use crate::launcher::RealLauncher;
use crate::render;

#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub dry_run: bool,
    pub yes: bool,
    pub allow_sensitive: bool,
    pub allow_dangerous: bool,
    pub only: Vec<String>,
    pub skip: Vec<String>,
}

pub fn run(app: &App, target: &str, opts: OpenOptions) -> Result<i32> {
    let mut resolution = app.resolve_target(target)?;
    resolution.apply_policy(&app.policy);

    if opts.dry_run {
        if app.json {
            println!("{}", serde_json::to_string_pretty(&resolution.to_json())?);
        } else {
            print!("{}", render::plan_text(&resolution));
            println!("(dry run: nothing was opened)");
        }
        return Ok(0);
    }

    if !app.json {
        print!("{}", render::plan_text(&resolution));
    }

    let approvals = consent::decide(&resolution, &opts);
    let approved = approvals
        .iter()
        .filter(|a| matches!(a, murl_core::dispatch::Approval::Approved))
        .count();
    crate::logger::info(&format!(
        "dispatching {approved} of {} planned resources",
        resolution.resources.len()
    ));
    let launcher = RealLauncher;
    let report = execute(&resolution, &approvals, &app.opener, &launcher, &app.limits)?;

    if app.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "resolution": resolution.to_json(),
                "report": report.to_json(),
            }))?
        );
    } else {
        println!();
        print!(
            "{}",
            render::report_text(&resolution.root().manifest.doc.name, &report)
        );
    }

    Ok(match report.aggregate {
        AggregateStatus::Success => 0,
        AggregateStatus::Partial => 3,
        AggregateStatus::Failed => 1,
        AggregateStatus::Denied => 4,
    })
}
