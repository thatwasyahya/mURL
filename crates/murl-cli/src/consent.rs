//! Consent: turning policy decisions plus user intent into approvals.
//!
//! The rules, in order (spec §8.3):
//!
//! 1. `--only` / `--skip` narrow the plan (skipped, not denied).
//! 2. A policy `Deny` is final — no flag and no prompt overrides it.
//! 3. A policy `Allow` dispatches without an individual prompt.
//! 4. A policy `Prompt` needs consent: an explicit flag scoped to the tier
//!    (`--yes` for SAFE, `--allow-sensitive`, `--allow-dangerous`), or an
//!    interactive answer. Without a TTY and without flags, the answer is no —
//!    a scheme handler invoked by a hostile document must fail closed.

use std::io::{BufRead, IsTerminal, Write};

use murl_core::dispatch::Approval;
use murl_core::policy::{Decision, Tier};
use murl_core::resolver::Resolution;

use crate::commands::open::OpenOptions;

pub fn decide(resolution: &Resolution, opts: &OpenOptions) -> Vec<Approval> {
    let mut approvals: Vec<Option<Approval>> = Vec::with_capacity(resolution.resources.len());
    let mut pending: Vec<usize> = Vec::new();

    for (i, pr) in resolution.resources.iter().enumerate() {
        // Narrowing first: --only matches resource ids or root anchors.
        if !opts.only.is_empty()
            && !opts
                .only
                .iter()
                .any(|s| s == &pr.resource.id || s == &pr.root_anchor)
        {
            approvals.push(Some(Approval::Skipped("not selected (--only)".into())));
            continue;
        }
        if opts.skip.iter().any(|s| s == &pr.resource.id) {
            approvals.push(Some(Approval::Skipped("skipped (--skip)".into())));
            continue;
        }

        match pr.decision.as_ref().expect("policy applied before consent") {
            Decision::Deny(reason) => {
                approvals.push(Some(Approval::Denied(reason.clone())));
            }
            Decision::Allow => approvals.push(Some(Approval::Approved)),
            Decision::Prompt(_) => {
                let flag_approved = match pr.tier {
                    Tier::Safe => opts.yes || opts.allow_sensitive || opts.allow_dangerous,
                    Tier::Sensitive => opts.allow_sensitive,
                    Tier::Dangerous => opts.allow_dangerous,
                };
                if flag_approved {
                    approvals.push(Some(Approval::Approved));
                } else {
                    approvals.push(None);
                    pending.push(i);
                }
            }
        }
    }

    if !pending.is_empty() {
        let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        if interactive {
            prompt_for(resolution, &pending, &mut approvals);
        } else {
            for &i in &pending {
                approvals[i] = Some(Approval::Denied(
                    "requires consent; re-run interactively or pass --yes / --allow-sensitive / --allow-dangerous"
                        .into(),
                ));
            }
        }
    }

    approvals
        .into_iter()
        .map(|a| a.expect("every slot decided"))
        .collect()
}

fn prompt_for(resolution: &Resolution, pending: &[usize], approvals: &mut [Option<Approval>]) {
    let mut err = std::io::stderr();
    let _ = writeln!(err, "\nThe following resources need your consent:");
    for &i in pending {
        let pr = &resolution.resources[i];
        let reasons = match pr.decision.as_ref() {
            Some(Decision::Prompt(r)) if !r.is_empty() => format!("  [{}]", r.join("; ")),
            _ => String::new(),
        };
        let _ = writeln!(
            err,
            "  {:10} {:9} {:9} {}{}",
            pr.resource.id,
            pr.tier.to_string(),
            pr.kind.to_string(),
            pr.resource.target,
            reasons
        );
    }
    let _ = write!(
        err,
        "\nOpen these? [a]ll / [s]elect individually / [N]one: "
    );
    let _ = err.flush();

    let answer = read_line().to_lowercase();
    match answer.trim() {
        "a" | "all" => {
            for &i in pending {
                approvals[i] = Some(Approval::Approved);
            }
        }
        "s" | "select" => {
            for &i in pending {
                let pr = &resolution.resources[i];
                let _ = write!(
                    err,
                    "  open {} ({} {} {})? [y/N]: ",
                    pr.resource.id, pr.tier, pr.kind, pr.resource.target
                );
                let _ = err.flush();
                let yes = matches!(read_line().trim().to_lowercase().as_str(), "y" | "yes");
                approvals[i] = Some(if yes {
                    Approval::Approved
                } else {
                    Approval::Denied("declined".into())
                });
            }
        }
        _ => {
            for &i in pending {
                approvals[i] = Some(Approval::Denied("declined".into()));
            }
        }
    }
}

fn read_line() -> String {
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line
}
