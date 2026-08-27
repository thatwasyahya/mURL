//! The policy engine: classification and consent decisions.
//!
//! Design rule (spec §8, `docs/security.md`): the *manifest author proposes,
//! the user's policy disposes*. Nothing in a manifest can grant itself
//! permission; manifests carry no permission grants at all. Classification is
//! derived from what a resource *is* (kind + target), and the local policy
//! decides what that classification requires: silent allow, explicit consent,
//! or refusal.

use serde::{Deserialize, Serialize};

use crate::kind::{has_executable_extension, Kind};
use crate::trust::TrustStatus;

/// Risk classification of a single resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Tier {
    /// Rendered by a sandboxed viewer (browser). Worst case ≈ opening a tab.
    Safe,
    /// Touches local data: files, directories.
    Sensitive,
    /// Opening it is (or can be one step from) code execution: terminals,
    /// executables, custom handlers.
    Dangerous,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Safe => f.write_str("SAFE"),
            Tier::Sensitive => f.write_str("SENSITIVE"),
            Tier::Dangerous => f.write_str("DANGEROUS"),
        }
    }
}

/// Classify a resource by kind and target. Pure function of the resource —
/// trust and origin are handled separately by [`Policy::evaluate`].
pub fn classify(kind: &Kind, target: &str) -> Tier {
    match kind {
        Kind::Https => Tier::Safe,
        // A `murl` resource is a container; it is never dispatched itself.
        // Its children are classified individually after splicing.
        Kind::Murl => Tier::Safe,
        Kind::Dir => Tier::Sensitive,
        Kind::File => {
            if has_executable_extension(target) {
                // "Opening" an executable or a .desktop file IS running it.
                Tier::Dangerous
            } else {
                Tier::Sensitive
            }
        }
        Kind::Terminal => Tier::Dangerous,
        Kind::Custom(_) => Tier::Dangerous,
    }
}

/// What the local policy requires for a tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsentMode {
    /// Dispatch without an individual prompt (still listed in the plan).
    Allow,
    /// Require explicit user consent.
    Prompt,
    /// Never dispatch.
    Deny,
}

/// Local consent policy. Defaults are the specification's recommended
/// baseline; anything more permissive is an explicit local decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Policy {
    pub safe: ConsentMode,
    pub sensitive: ConsentMode,
    pub dangerous: ConsentMode,
    /// DANGEROUS resources additionally require the manifest to be trusted
    /// (local, or signed by a key pinned for its authority). Consent alone is
    /// not enough: a user can be talked into one click, and one click is
    /// exactly what an untrusted mURL gets.
    pub dangerous_requires_trust: bool,
    /// Consent mode applied on top when a *remotely fetched* manifest
    /// references the local filesystem — a remote author asserting knowledge
    /// of your disk is inherently suspicious.
    pub remote_filesystem_refs: ConsentMode,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            safe: ConsentMode::Prompt,
            sensitive: ConsentMode::Prompt,
            dangerous: ConsentMode::Prompt,
            dangerous_requires_trust: true,
            remote_filesystem_refs: ConsentMode::Prompt,
        }
    }
}

/// Facts about the manifest a resource came from, needed for evaluation.
#[derive(Debug, Clone)]
pub struct EvalContext {
    pub trust: TrustStatus,
    pub manifest_expired: bool,
    /// True when the manifest was fetched from a remote authority (as opposed
    /// to the local store or an explicit local file).
    pub remote_origin: bool,
}

/// The policy verdict for one resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "decision", content = "reasons")]
pub enum Decision {
    /// May dispatch without individual consent.
    Allow,
    /// Requires explicit consent; reasons explain why.
    Prompt(Vec<String>),
    /// Refused; the single reason explains why. Consent cannot override.
    Deny(String),
}

impl Policy {
    pub fn evaluate(&self, kind: &Kind, tier: Tier, ctx: &EvalContext) -> Decision {
        // Hard gates first: these cannot be consented through.
        if tier == Tier::Dangerous && self.dangerous_requires_trust && !ctx.trust.is_trusted() {
            return Decision::Deny(format!(
                "DANGEROUS resource from an untrusted manifest ({}); pin the signing key with `murl trust add` or install the manifest locally",
                ctx.trust
            ));
        }
        if ctx.manifest_expired && tier != Tier::Safe {
            return Decision::Deny("manifest is expired".into());
        }

        let mode = match tier {
            Tier::Safe => self.safe,
            Tier::Sensitive => self.sensitive,
            Tier::Dangerous => self.dangerous,
        };
        if mode == ConsentMode::Deny {
            return Decision::Deny(format!("{tier} resources are disabled by policy"));
        }

        let mut reasons: Vec<String> = Vec::new();
        if mode == ConsentMode::Prompt {
            reasons.push(format!("{tier} resource"));
        }
        if ctx.manifest_expired {
            reasons.push("manifest is expired".into());
        }
        if ctx.remote_origin && kind.is_filesystem() {
            match self.remote_filesystem_refs {
                ConsentMode::Deny => {
                    return Decision::Deny(
                        "remote manifests may not reference the local filesystem (policy)".into(),
                    )
                }
                ConsentMode::Prompt => {
                    reasons.push("remote manifest references the local filesystem".into())
                }
                ConsentMode::Allow => {}
            }
        }
        if tier == Tier::Dangerous && !ctx.trust.is_trusted() {
            // Only reachable when dangerous_requires_trust was disabled.
            reasons.push("dangerous resource from an untrusted manifest".into());
        }

        if reasons.is_empty() {
            Decision::Allow
        } else {
            Decision::Prompt(reasons)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(trust: TrustStatus, expired: bool, remote: bool) -> EvalContext {
        EvalContext {
            trust,
            manifest_expired: expired,
            remote_origin: remote,
        }
    }

    #[test]
    fn classification_matrix() {
        assert_eq!(classify(&Kind::Https, "https://e.com"), Tier::Safe);
        assert_eq!(classify(&Kind::Dir, "/home/u/p"), Tier::Sensitive);
        assert_eq!(classify(&Kind::File, "/home/u/a.pdf"), Tier::Sensitive);
        assert_eq!(classify(&Kind::File, "/home/u/a.sh"), Tier::Dangerous);
        assert_eq!(classify(&Kind::File, "/home/u/a.desktop"), Tier::Dangerous);
        assert_eq!(classify(&Kind::Terminal, "/home/u/p"), Tier::Dangerous);
        assert_eq!(
            classify(&Kind::Custom("x".into()), "anything"),
            Tier::Dangerous
        );
    }

    #[test]
    fn dangerous_from_untrusted_is_denied_not_prompted() {
        let p = Policy::default();
        let d = p.evaluate(
            &Kind::Terminal,
            Tier::Dangerous,
            &ctx(TrustStatus::UnsignedRemote, false, true),
        );
        assert!(matches!(d, Decision::Deny(_)));
        // Even a signature from an unknown key is not trust.
        let d = p.evaluate(
            &Kind::Terminal,
            Tier::Dangerous,
            &ctx(
                TrustStatus::SignedUnknownKey {
                    key_id: "ed25519:aabbccdd00112233".into(),
                },
                false,
                true,
            ),
        );
        assert!(matches!(d, Decision::Deny(_)));
    }

    #[test]
    fn dangerous_from_trusted_prompts() {
        let p = Policy::default();
        let d = p.evaluate(
            &Kind::Terminal,
            Tier::Dangerous,
            &ctx(TrustStatus::Local, false, false),
        );
        assert!(matches!(d, Decision::Prompt(_)));
    }

    #[test]
    fn expired_blocks_non_safe() {
        let p = Policy::default();
        let d = p.evaluate(
            &Kind::File,
            Tier::Sensitive,
            &ctx(TrustStatus::Local, true, false),
        );
        assert!(matches!(d, Decision::Deny(_)));
        let d = p.evaluate(
            &Kind::Https,
            Tier::Safe,
            &ctx(TrustStatus::Local, true, false),
        );
        assert!(matches!(d, Decision::Prompt(ref r) if r.iter().any(|s| s.contains("expired"))));
    }

    #[test]
    fn remote_filesystem_refs_add_prompt_reason() {
        let p = Policy {
            safe: ConsentMode::Allow,
            sensitive: ConsentMode::Allow,
            ..Policy::default()
        };
        let d = p.evaluate(
            &Kind::File,
            Tier::Sensitive,
            &ctx(
                TrustStatus::SignedTrusted {
                    key_id: "ed25519:0011223344556677".into(),
                },
                false,
                true,
            ),
        );
        assert!(matches!(d, Decision::Prompt(ref r) if r.iter().any(|s| s.contains("filesystem"))));
        // Same resource from a local manifest sails through under Allow.
        let d = p.evaluate(
            &Kind::File,
            Tier::Sensitive,
            &ctx(TrustStatus::Local, false, false),
        );
        assert_eq!(d, Decision::Allow);
    }

    #[test]
    fn deny_mode_is_final() {
        let p = Policy {
            dangerous: ConsentMode::Deny,
            ..Policy::default()
        };
        let d = p.evaluate(
            &Kind::Terminal,
            Tier::Dangerous,
            &ctx(TrustStatus::Local, false, false),
        );
        assert!(matches!(d, Decision::Deny(_)));
    }
}
