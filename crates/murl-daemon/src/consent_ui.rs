//! The consent surface, as an abstraction.
//!
//! The daemon exists to give consent a better surface than a terminal; the
//! *security* of consent must not depend on which surface is used. So the
//! rules live here, once, and only the presentation varies:
//!
//! * A policy `Deny` is never shown as approvable. The user cannot click
//!   past a refusal (threat D-2: a spoofed dialog gains nothing an attacker
//!   could not already ask for).
//! * A policy `Allow` needs no interaction.
//! * Everything else must be granted explicitly, per resource.
//! * Any failure to present — no display, closed dialog, timeout — is a
//!   denial, never an approval.

use murl_core::dispatch::Approval;
use murl_core::policy::{Decision, Tier};
use murl_core::resolver::Resolution;

/// One resource as offered to the user.
#[derive(Debug, Clone)]
pub struct ConsentItem {
    pub index: usize,
    pub id: String,
    pub label: String,
    pub kind: String,
    pub target: String,
    pub tier: Tier,
    pub reasons: Vec<String>,
}

/// What a surface is asked to present.
#[derive(Debug, Clone)]
pub struct ConsentRequest {
    /// Destination name, e.g. "Project X".
    pub name: String,
    /// Canonical identity, when resolved by name.
    pub identity: Option<String>,
    /// Where the manifest came from, already described.
    pub origin: String,
    /// Trust status, already described.
    pub trust: String,
    /// Resources needing an explicit decision.
    pub items: Vec<ConsentItem>,
    /// Resources refused by policy — shown, never offered.
    pub denied: Vec<(ConsentItem, String)>,
}

/// A surface that can ask the user. Implementations: terminal (today),
/// per-platform GUI (as it lands).
pub trait ConsentUi: std::fmt::Debug {
    /// Return the indices the user granted. Implementations MUST return an
    /// empty set when they cannot present, and MUST NOT return an index
    /// that was not offered.
    fn ask(&self, request: &ConsentRequest) -> Vec<usize>;
}

/// Build the request and the pre-decided approvals from a resolution whose
/// policy has already been applied.
///
/// Returns the request to present plus a slot per planned resource:
/// `Some(approval)` where policy already decided, `None` where the user
/// must be asked (matching a [`ConsentItem`] by index).
pub fn prepare(
    resolution: &Resolution,
    only: &[String],
) -> (ConsentRequest, Vec<Option<Approval>>) {
    let root = resolution.root();
    let mut slots: Vec<Option<Approval>> = Vec::with_capacity(resolution.resources.len());
    let mut items = Vec::new();
    let mut denied = Vec::new();

    for (index, pr) in resolution.resources.iter().enumerate() {
        let item = ConsentItem {
            index,
            id: pr.resource.id.clone(),
            label: pr.display_label().to_owned(),
            kind: pr.kind.to_string(),
            target: pr.resource.target.clone(),
            tier: pr.tier,
            reasons: Vec::new(),
        };

        // A client's `only` list can narrow, never widen.
        if !only.is_empty()
            && !only
                .iter()
                .any(|s| s == &pr.resource.id || s == &pr.root_anchor)
        {
            slots.push(Some(Approval::Skipped("not selected".into())));
            continue;
        }

        match pr.decision.as_ref() {
            Some(Decision::Allow) => slots.push(Some(Approval::Approved)),
            Some(Decision::Deny(reason)) => {
                denied.push((item, reason.clone()));
                slots.push(Some(Approval::Denied(reason.clone())));
            }
            Some(Decision::Prompt(reasons)) => {
                slots.push(None);
                items.push(ConsentItem {
                    reasons: reasons.clone(),
                    ..item
                });
            }
            None => {
                // apply_policy was not run: fail closed rather than guess.
                slots.push(Some(Approval::Denied("no policy decision".into())));
            }
        }
    }

    let request = ConsentRequest {
        name: root.manifest.doc.name.clone(),
        identity: root.identity.clone(),
        origin: root.origin.describe(),
        trust: root.trust.to_string(),
        items,
        denied,
    };
    (request, slots)
}

/// Fold the user's granted indices into the approval slots.
///
/// Only slots left undecided by policy can be granted — a surface that
/// returns indices it was never offered cannot resurrect a denial.
pub fn apply(mut slots: Vec<Option<Approval>>, granted: &[usize]) -> Vec<Approval> {
    for (i, slot) in slots.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(if granted.contains(&i) {
                Approval::Approved
            } else {
                Approval::Denied("not granted".into())
            });
        }
    }
    slots
        .into_iter()
        .map(|s| s.expect("every slot decided"))
        .collect()
}

/// A surface that grants nothing. The default when no display is available
/// and the safe answer to "what if the UI fails?".
#[derive(Debug, Default)]
pub struct DenyAllUi;

impl ConsentUi for DenyAllUi {
    fn ask(&self, _request: &ConsentRequest) -> Vec<usize> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct GrantAll;
    impl ConsentUi for GrantAll {
        fn ask(&self, request: &ConsentRequest) -> Vec<usize> {
            request.items.iter().map(|i| i.index).collect()
        }
    }

    #[derive(Debug)]
    struct GrantAnything;
    impl ConsentUi for GrantAnything {
        fn ask(&self, _r: &ConsentRequest) -> Vec<usize> {
            (0..1000).collect()
        }
    }

    fn empty_request() -> ConsentRequest {
        ConsentRequest {
            name: "T".into(),
            identity: None,
            origin: "test".into(),
            trust: "LOCAL".into(),
            items: vec![],
            denied: vec![],
        }
    }

    #[test]
    fn ungranted_slots_become_denials() {
        let approvals = apply(vec![None; 3], &[0, 2]);
        assert_eq!(approvals[0], Approval::Approved);
        assert!(matches!(approvals[1], Approval::Denied(_)));
        assert_eq!(approvals[2], Approval::Approved);
    }

    #[test]
    fn deny_all_ui_grants_nothing() {
        let mut request = empty_request();
        request.items.push(ConsentItem {
            index: 0,
            id: "a".into(),
            label: "A".into(),
            kind: "https".into(),
            target: "https://e.com".into(),
            tier: Tier::Safe,
            reasons: vec!["SAFE resource".into()],
        });
        assert!(DenyAllUi.ask(&request).is_empty());
        assert_eq!(GrantAll.ask(&request), vec![0]);
    }

    #[test]
    fn a_ui_cannot_grant_slots_it_was_not_offered() {
        // Slot 1 was already decided (denied by policy); a rogue surface
        // returning every index must not resurrect it.
        let mut pre = vec![None, Some(Approval::Denied("policy".into())), None];
        pre[2] = Some(Approval::Skipped("not selected".into()));
        let granted = GrantAnything.ask(&empty_request());
        let approvals = apply(pre, &granted);
        assert_eq!(approvals[0], Approval::Approved);
        assert!(matches!(approvals[1], Approval::Denied(_)));
        assert!(matches!(approvals[2], Approval::Skipped(_)));
    }
}
