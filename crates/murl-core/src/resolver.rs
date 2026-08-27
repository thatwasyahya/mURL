//! The resolver: mURL → resolved, policy-annotated resource plan.
//!
//! Pipeline (spec §6, `docs/resolution.md`):
//!
//! ```text
//! murl string
//!   → parse                (murl.rs)
//!   → locate manifest      (local store | cache | remote fetcher)
//!   → size cap + parse     (manifest.rs)
//!   → validate             (manifest.rs)
//!   → verify signature     (trust.rs)      — invalid signature: hard stop
//!   → bind identity        (§6.4)          — manifest.id must match the name
//!   → splice nested mURLs  (recursively, under depth/count/cycle limits)
//!   → classify + evaluate  (policy.rs)
//!   → Resolution           (input to consent + dispatch)
//! ```
//!
//! Everything here is deterministic given the fetcher/store/clock, which is
//! what makes the security tests in `tests/` meaningful.

use std::collections::HashSet;

use serde_json::json;

use crate::cache::ManifestCache;
use crate::error::{Error, Result};
use crate::fetch::{well_known_url, LocalStore, RemoteFetcher};
use crate::kind::Kind;
use crate::limits::Limits;
use crate::manifest::{Manifest, ResourceDoc};
use crate::murl::{Murl, VersionTag};
use crate::policy::{classify, Decision, EvalContext, Policy, Tier};
use crate::time::{parse_rfc3339_utc, Clock};
use crate::trust::{check_integrity, verify_manifest, TrustStatus, TrustStore};

/// Where a manifest came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    LocalStore(std::path::PathBuf),
    LocalFile(std::path::PathBuf),
    Remote { url: String, from_cache: bool },
}

impl Origin {
    pub fn is_remote(&self) -> bool {
        matches!(self, Origin::Remote { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Origin::LocalStore(p) => format!("local store ({})", p.display()),
            Origin::LocalFile(p) => format!("file ({})", p.display()),
            Origin::Remote {
                url,
                from_cache: true,
            } => format!("cache ({url})"),
            Origin::Remote {
                url,
                from_cache: false,
            } => format!("remote ({url})"),
        }
    }
}

/// One resolved manifest in the tree.
#[derive(Debug, Clone)]
pub struct ResolvedNode {
    /// Canonical identity, when resolved by name (`None` for direct files).
    pub identity: Option<String>,
    pub origin: Origin,
    pub manifest: Manifest,
    pub trust: TrustStatus,
    pub depth: u32,
    pub expired: bool,
}

/// One dispatchable resource in the flattened plan.
#[derive(Debug, Clone)]
pub struct PlannedResource {
    pub resource: ResourceDoc,
    pub kind: Kind,
    pub tier: Tier,
    /// Index into [`Resolution::nodes`].
    pub node: usize,
    /// The id of the *root-manifest* resource this came from — the unit the
    /// `#selector` fragment addresses.
    pub root_anchor: String,
    /// Filled by [`Resolution::apply_policy`].
    pub decision: Option<Decision>,
}

impl PlannedResource {
    pub fn display_label(&self) -> &str {
        self.resource.label.as_deref().unwrap_or(&self.resource.id)
    }
}

/// The complete result of resolving an mURL.
#[derive(Debug)]
pub struct Resolution {
    pub nodes: Vec<ResolvedNode>,
    pub resources: Vec<PlannedResource>,
    pub warnings: Vec<String>,
    pub selector: Option<String>,
}

impl Resolution {
    /// The root manifest.
    pub fn root(&self) -> &ResolvedNode {
        &self.nodes[0]
    }

    /// Run the policy engine over every planned resource.
    pub fn apply_policy(&mut self, policy: &Policy) {
        for pr in &mut self.resources {
            let node = &self.nodes[pr.node];
            let ctx = EvalContext {
                trust: node.trust.clone(),
                manifest_expired: node.expired,
                remote_origin: node.origin.is_remote(),
            };
            pr.decision = Some(policy.evaluate(&pr.kind, pr.tier, &ctx));
        }
    }

    /// Machine-readable form for `--json` output and tooling.
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "selector": self.selector,
            "warnings": self.warnings,
            "nodes": self.nodes.iter().map(|n| json!({
                "identity": n.identity,
                "origin": n.origin.describe(),
                "remote": n.origin.is_remote(),
                "name": n.manifest.doc.name,
                "trust": n.trust,
                "depth": n.depth,
                "expired": n.expired,
            })).collect::<Vec<_>>(),
            "resources": self.resources.iter().map(|pr| json!({
                "id": pr.resource.id,
                "label": pr.display_label(),
                "kind": pr.kind.to_string(),
                "target": pr.resource.target,
                "role": pr.resource.role,
                "required": pr.resource.required,
                "tier": pr.tier,
                "node": pr.node,
                "rootAnchor": pr.root_anchor,
                "decision": pr.decision,
            })).collect::<Vec<_>>(),
        })
    }
}

/// The resolver. Borrowed collaborators keep it cheap to construct per
/// operation and trivially testable.
#[derive(Debug)]
pub struct Resolver<'a> {
    pub local_store: &'a LocalStore,
    pub remote: Option<&'a dyn RemoteFetcher>,
    pub cache: Option<&'a ManifestCache>,
    pub trust_store: &'a TrustStore,
    pub limits: Limits,
    pub clock: &'a dyn Clock,
}

#[derive(Default)]
struct State {
    nodes: Vec<ResolvedNode>,
    resources: Vec<PlannedResource>,
    warnings: Vec<String>,
    /// Identities on the current resolution path (cycle detection).
    stack: Vec<String>,
    /// Identities fully resolved anywhere in the tree (DAG dedup).
    visited: HashSet<String>,
    /// (kind, target) pairs already planned (duplicate suppression).
    dedup: HashSet<(String, String)>,
    manifests: usize,
}

impl<'a> Resolver<'a> {
    /// Resolve a named mURL.
    pub fn resolve(&self, murl: &Murl) -> Result<Resolution> {
        let mut st = State::default();
        st.stack.push(murl.identity());
        self.ingest_named(&mut st, murl, 0, None, None)?;
        st.stack.pop();
        self.finish(st, murl.selector.clone())
    }

    /// Fetch just the root manifest for a name, without validation or
    /// recursive splicing. Used by inspection tooling (`murl validate`,
    /// `murl inspect`, `murl verify`) that must examine a manifest exactly
    /// as published — including one that would fail full resolution.
    pub fn fetch_root(&self, murl: &Murl) -> Result<(Manifest, Origin, Vec<String>)> {
        let mut st = State::default();
        let (bytes, origin) = self.locate(&mut st, murl)?;
        let manifest = Manifest::from_slice(&bytes, &self.limits)?;
        Ok((manifest, origin, st.warnings))
    }

    /// Resolve a manifest from an explicit local file (no name involved).
    pub fn resolve_file(&self, path: &std::path::Path) -> Result<Resolution> {
        let bytes = std::fs::read(path).map_err(|e| {
            Error::NotFound(format!("cannot read manifest {}: {e}", path.display()))
        })?;
        let mut st = State::default();
        self.ingest_bytes(
            &mut st,
            None,
            &bytes,
            Origin::LocalFile(path.to_path_buf()),
            0,
            None,
        )?;
        self.finish(st, None)
    }

    fn finish(&self, st: State, selector: Option<String>) -> Result<Resolution> {
        let mut resolution = Resolution {
            nodes: st.nodes,
            resources: st.resources,
            warnings: st.warnings,
            selector: selector.clone(),
        };
        if let Some(sel) = &selector {
            let root_has = resolution.nodes[0]
                .manifest
                .doc
                .resources
                .iter()
                .any(|r| &r.id == sel);
            if !root_has {
                return Err(Error::NotFound(format!(
                    "selector `#{sel}` does not match any resource id in the root manifest"
                )));
            }
            resolution.resources.retain(|pr| &pr.root_anchor == sel);
        }
        Ok(resolution)
    }

    fn ingest_named(
        &self,
        st: &mut State,
        murl: &Murl,
        depth: u32,
        anchor: Option<String>,
        integrity: Option<&str>,
    ) -> Result<()> {
        st.manifests += 1;
        if st.manifests > self.limits.max_nested_manifests {
            return Err(Error::LimitExceeded(format!(
                "resolution touches more than {} manifests",
                self.limits.max_nested_manifests
            )));
        }

        let (bytes, origin) = self.locate(st, murl)?;
        if let Some(pin) = integrity {
            check_integrity(&bytes, pin)?;
        }
        self.ingest_bytes(st, Some(murl), &bytes, origin, depth, anchor)
    }

    /// Locate manifest bytes for a named mURL: local store, else cache,
    /// else network, else stale cache as an explicit offline fallback.
    fn locate(&self, st: &mut State, murl: &Murl) -> Result<(Vec<u8>, Origin)> {
        if murl.authority.is_local() {
            let (bytes, path) = self.local_store.load(murl)?;
            return Ok((bytes, Origin::LocalStore(path)));
        }

        let identity = murl.identity();
        let ttl = match murl.version {
            VersionTag::Latest => self.limits.cache_ttl_secs,
            VersionTag::Pinned(_) => u64::MAX,
        };
        let now = self.clock.now_epoch();
        let cached = self.cache.and_then(|c| c.get(&identity, ttl, now));

        if let Some(entry) = &cached {
            if entry.fresh {
                return Ok((
                    entry.bytes.clone(),
                    Origin::Remote {
                        url: entry.meta.url.clone(),
                        from_cache: true,
                    },
                ));
            }
        }

        let url = well_known_url(murl).expect("remote authority has a well-known URL");
        match self.remote {
            Some(fetcher) => match fetcher.fetch(&url, &self.limits) {
                Ok(bytes) => {
                    if bytes.len() > self.limits.max_manifest_bytes {
                        return Err(Error::LimitExceeded(format!(
                            "manifest at {url} exceeds {} bytes",
                            self.limits.max_manifest_bytes
                        )));
                    }
                    if let Some(cache) = self.cache {
                        if let Err(e) = cache.put(&identity, &url, &bytes, now) {
                            st.warnings.push(ManifestCache::describe_err(&e));
                        }
                    }
                    Ok((bytes, Origin::Remote { url, from_cache: false }))
                }
                Err(fetch_err) => match cached {
                    Some(entry) => {
                        st.warnings.push(format!(
                            "{identity}: fetch failed ({fetch_err}); using stale cache from {}",
                            entry.meta.url
                        ));
                        Ok((
                            entry.bytes.clone(),
                            Origin::Remote { url: entry.meta.url.clone(), from_cache: true },
                        ))
                    }
                    None => Err(fetch_err),
                },
            },
            None => match cached {
                Some(entry) => {
                    st.warnings.push(format!(
                        "{identity}: offline; using cached manifest from {}",
                        entry.meta.url
                    ));
                    Ok((
                        entry.bytes.clone(),
                        Origin::Remote { url: entry.meta.url.clone(), from_cache: true },
                    ))
                }
                None => Err(Error::Resolution(format!(
                    "`{identity}` requires network resolution, but the resolver is offline and the manifest is not cached"
                ))),
            },
        }
    }

    fn ingest_bytes(
        &self,
        st: &mut State,
        murl: Option<&Murl>,
        bytes: &[u8],
        origin: Origin,
        depth: u32,
        anchor: Option<String>,
    ) -> Result<()> {
        let label = murl
            .map(Murl::identity)
            .unwrap_or_else(|| origin.describe());

        let manifest = Manifest::from_slice(bytes, &self.limits)?;
        let report = manifest.validate();
        for w in &report.warnings {
            st.warnings
                .push(format!("{label}: {} — {}", w.path, w.message));
        }
        report.into_result()?;

        // Signature and trust. Invalid signature is a hard stop inside
        // verify_manifest.
        let verified = verify_manifest(&manifest.raw)?;
        let trust = match &origin {
            Origin::LocalStore(_) | Origin::LocalFile(_) => TrustStatus::Local,
            Origin::Remote { .. } => {
                let authority = murl.map(|m| m.authority.to_string()).unwrap_or_default();
                match &verified {
                    None => TrustStatus::UnsignedRemote,
                    Some(v) => {
                        if self.trust_store.is_trusted(&authority, &v.key_id) {
                            TrustStatus::SignedTrusted {
                                key_id: v.key_id.clone(),
                            }
                        } else {
                            TrustStatus::SignedUnknownKey {
                                key_id: v.key_id.clone(),
                            }
                        }
                    }
                }
            }
        };
        if verified.is_some() && manifest.doc.id.is_none() {
            st.warnings.push(format!(
                "{label}: signed manifest carries no `id`; without it a valid signature can be replayed under a different name"
            ));
        }

        // Identity binding (§6.4): a manifest that declares an id must have
        // been requested under that id.
        if let (Some(requested), Some(declared)) = (murl, &manifest.doc.id) {
            let declared = Murl::parse(declared)
                .map_err(|e| Error::Validation(format!("{label}: manifest id is invalid: {e}")))?;
            let name_matches =
                declared.authority == requested.authority && declared.name == requested.name;
            let version_conflicts = matches!(
                (&declared.version, &requested.version),
                (VersionTag::Pinned(a), VersionTag::Pinned(b)) if a != b
            );
            if !name_matches || version_conflicts {
                return Err(Error::Resolution(format!(
                    "manifest at {} declares id `{}` but was resolved as `{}` — refusing a re-labeled manifest",
                    origin.describe(),
                    declared.identity(),
                    requested.identity()
                )));
            }
        }

        // Expiry.
        let expired = match &manifest.doc.expires {
            Some(exp) => {
                // Format already validated.
                let when = parse_rfc3339_utc(exp).unwrap_or(0);
                let expired = when <= self.clock.now_epoch();
                if expired {
                    st.warnings
                        .push(format!("{label}: manifest expired at {exp}"));
                }
                expired
            }
            None => false,
        };

        let node_index = st.nodes.len();
        st.nodes.push(ResolvedNode {
            identity: murl.map(Murl::identity),
            origin,
            manifest: manifest.clone(),
            trust,
            depth,
            expired,
        });

        let order =
            crate::graph::execution_order(&manifest.doc.resources).map_err(Error::Validation)?;

        for idx in order {
            let r = &manifest.doc.resources[idx];
            let kind = Kind::parse(&r.kind).map_err(Error::Validation)?;
            let this_anchor = anchor.clone().unwrap_or_else(|| r.id.clone());

            if let Kind::Murl = kind {
                let child = Murl::parse(&r.target)
                    .map_err(|e| Error::Validation(format!("nested mURL: {e}")))?;
                if child.selector.is_some() {
                    st.warnings.push(format!(
                        "{label}: selector on nested mURL `{}` is ignored",
                        r.target
                    ));
                }
                if depth + 1 > self.limits.max_depth {
                    return Err(Error::LimitExceeded(format!(
                        "nesting depth exceeds {} at `{}`",
                        self.limits.max_depth,
                        child.identity()
                    )));
                }
                let identity = child.identity();
                if st.stack.iter().any(|s| s == &identity) {
                    return Err(Error::Cycle(format!(
                        "{} -> {identity}",
                        st.stack.join(" -> ")
                    )));
                }
                if st.visited.contains(&identity) {
                    st.warnings.push(format!(
                        "{label}: nested mURL `{identity}` already resolved elsewhere; skipping duplicate"
                    ));
                    continue;
                }
                if let Some(parent) = murl {
                    if parent.authority != child.authority {
                        st.warnings.push(format!(
                            "{label}: nested mURL `{identity}` crosses into authority `{}`",
                            child.authority
                        ));
                    }
                }
                st.stack.push(identity.clone());
                self.ingest_named(
                    st,
                    &child,
                    depth + 1,
                    Some(this_anchor),
                    r.integrity.as_deref(),
                )?;
                st.stack.pop();
                st.visited.insert(identity);
                continue;
            }

            if st.resources.len() + 1 > self.limits.max_total_resources {
                return Err(Error::LimitExceeded(format!(
                    "resolution exceeds {} total resources",
                    self.limits.max_total_resources
                )));
            }
            let dedup_key = (kind.to_string(), r.target.clone());
            if !st.dedup.insert(dedup_key) {
                st.warnings.push(format!(
                    "{label}: duplicate resource ({} {}) skipped",
                    kind, r.target
                ));
                continue;
            }
            let tier = classify(&kind, &r.target);
            st.resources.push(PlannedResource {
                resource: r.clone(),
                kind,
                tier,
                node: node_index,
                root_anchor: this_anchor,
                decision: None,
            });
        }
        Ok(())
    }
}
