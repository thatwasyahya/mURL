//! Human-readable rendering of plans and execution reports.
//!
//! All human output goes through here so `resolve` and `open` stay
//! consistent; machine output is the `--json` path and never mixes with this.

use murl_core::dispatch::{ExecutionReport, OutcomeStatus};
use murl_core::policy::Decision;
use murl_core::resolver::Resolution;

/// Render the resolution as a plan tree.
pub fn plan_text(resolution: &Resolution) -> String {
    let mut out = String::new();
    let root = resolution.root();
    out.push_str(&format!(
        "{}{}\n",
        root.manifest.doc.name,
        root.identity
            .as_ref()
            .map(|i| format!("  ({i})"))
            .unwrap_or_default()
    ));
    if let Some(desc) = &root.manifest.doc.description {
        out.push_str(&format!("  {desc}\n"));
    }
    for node in &resolution.nodes {
        out.push_str(&format!(
            "  manifest: {}  trust: {}{}\n",
            node.origin.describe(),
            node.trust,
            if node.expired { "  [EXPIRED]" } else { "" }
        ));
    }
    if let Some(sel) = &resolution.selector {
        out.push_str(&format!("  selector: #{sel}\n"));
    }

    out.push_str("\n  resources:\n");
    if resolution.resources.is_empty() {
        out.push_str("    (none)\n");
    }
    let id_width = resolution
        .resources
        .iter()
        .map(|p| p.resource.id.len())
        .max()
        .unwrap_or(0);
    let n = resolution.resources.len();
    for (i, pr) in resolution.resources.iter().enumerate() {
        let branch = if i + 1 == n { "└─" } else { "├─" };
        let mut line = format!(
            "    {branch} {:id_width$}  {:9}  {:9}  {}",
            pr.resource.id,
            pr.tier.to_string(),
            pr.kind.to_string(),
            pr.resource.target,
        );
        match &pr.decision {
            Some(Decision::Deny(reason)) => {
                line.push_str(&format!("\n         ✗ denied: {reason}"))
            }
            Some(Decision::Prompt(reasons)) if !reasons.is_empty() => {
                line.push_str(&format!("\n         ⚠ consent: {}", reasons.join("; ")));
            }
            _ => {}
        }
        out.push_str(&line);
        out.push('\n');
    }

    if !resolution.warnings.is_empty() {
        out.push_str("\n  warnings:\n");
        for w in &resolution.warnings {
            out.push_str(&format!("    ⚠ {w}\n"));
        }
    }
    out
}

/// Render the execution report.
pub fn report_text(name: &str, report: &ExecutionReport) -> String {
    let mut out = format!("{name}: {}\n", report.aggregate);
    let id_width = report
        .outcomes
        .iter()
        .map(|o| o.id.len())
        .max()
        .unwrap_or(0);
    for o in &report.outcomes {
        let symbol = match o.status {
            OutcomeStatus::Opened => "✓",
            OutcomeStatus::Skipped => "•",
            OutcomeStatus::Unavailable => "⚠",
            OutcomeStatus::Denied | OutcomeStatus::Failed => "✗",
            // OutcomeStatus is #[non_exhaustive] (docs/stability.md): a
            // future variant must render as *something*, not fail to build.
            _ => "?",
        };
        out.push_str(&format!(
            "  {symbol} {:id_width$}  {}{}\n",
            o.id,
            o.status,
            o.detail
                .as_ref()
                .map(|d| format!(" ({d})"))
                .unwrap_or_default()
        ));
    }
    out
}
