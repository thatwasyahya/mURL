//! Resolution and dispatch limits.
//!
//! Every limit here is a security control (see `docs/threat-model.md`,
//! threats T-7 "resource explosion" and T-8 "recursive mURL bombs"). They are
//! configurable so that operators can tighten them, but the defaults are the
//! specification's recommended values and loosening them is a policy decision,
//! not a convenience knob.

use serde::{Deserialize, Serialize};

/// Hard limits applied during resolution and dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    /// Maximum nesting depth of recursive `murl` resources. The root manifest
    /// is depth 0.
    pub max_depth: u32,
    /// Maximum number of dispatchable resources across the whole resolution,
    /// after splicing nested manifests.
    pub max_total_resources: usize,
    /// Maximum number of manifests fetched in one resolution (root included).
    pub max_nested_manifests: usize,
    /// Maximum size of a single manifest in bytes, enforced before parsing.
    pub max_manifest_bytes: usize,
    /// Per-fetch network timeout, seconds.
    pub fetch_timeout_secs: u64,
    /// Cache freshness window for `@latest` resolutions, seconds. Pinned
    /// versions are immutable and cached indefinitely.
    pub cache_ttl_secs: u64,
    /// Delay inserted between consecutive resource launches, milliseconds.
    /// Serialized dispatch keeps a 60-resource mURL from fork-bombing a
    /// desktop session.
    pub dispatch_stagger_ms: u64,
    /// HTTP redirects followed during manifest resolution. The specification
    /// requires 0: a manifest lives where its authority says it lives.
    pub max_redirects: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_depth: 3,
            max_total_resources: 64,
            max_nested_manifests: 8,
            max_manifest_bytes: 256 * 1024,
            fetch_timeout_secs: 10,
            cache_ttl_secs: 3600,
            dispatch_stagger_ms: 150,
            max_redirects: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let l = Limits::default();
        assert_eq!(l.max_depth, 3);
        assert_eq!(l.max_total_resources, 64);
        assert_eq!(l.max_manifest_bytes, 262_144);
        assert_eq!(l.max_redirects, 0);
    }
}
