//! The mURL manifest: parsing and validation.
//!
//! A manifest is the document an mURL resolves to. It is JSON
//! (`application/murl+json`, extension `.murl.json`) and is treated as
//! hostile input everywhere in this crate: size-capped before parsing,
//! strictly validated after, and interpreted only through the typed model.
//!
//! The parsed [`Manifest`] keeps *both* the typed document and the raw
//! [`serde_json::Value`]. Signatures cover the canonical form of the raw
//! document, so unknown members added by future spec versions remain covered
//! by the signature even though this implementation does not interpret them.
//! Re-serializing the typed struct and signing *that* would silently strip
//! such members — a forward-compatibility hazard the dual representation
//! avoids.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::kind::Kind;
use crate::limits::Limits;
use crate::murl::Murl;
use crate::time::parse_rfc3339_utc;

/// The format version this implementation writes.
pub const SUPPORTED_MURL_VERSION: &str = "0.2";
/// Format versions this implementation accepts (0.2 is additive over 0.1).
pub const ACCEPTED_MURL_VERSIONS: &[&str] = &["0.1", "0.2"];

pub const MAX_RESOURCES_PER_MANIFEST: usize = 64;
pub const MAX_NAME_LEN: usize = 120;
pub const MAX_DESCRIPTION_LEN: usize = 2000;
pub const MAX_TARGET_LEN: usize = 2048;
pub const MAX_LABEL_LEN: usize = 120;
pub const MAX_TAGS: usize = 16;
pub const MAX_TAG_LEN: usize = 32;
pub const MAX_DEPENDS: usize = 16;
pub const MAX_RELATIONS: usize = 128;

/// A parsed manifest: raw JSON plus the typed view over it.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// The manifest exactly as parsed — the thing signatures cover.
    pub raw: Value,
    /// The typed interpretation used by the rest of the pipeline.
    pub doc: ManifestDoc,
}

/// Typed manifest document (spec §5).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDoc {
    /// Spec version, e.g. `"0.1"`.
    pub murl_version: String,
    /// Optional canonical mURL this manifest is bound to. When present, the
    /// resolver enforces that the manifest was fetched under this identity —
    /// a signed manifest cannot be replayed under a different name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Human-readable destination name.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Manifest content version (dotted integers, e.g. `"1.4.2"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Strict UTC timestamp before which the manifest must not be used.
    /// With `expires`, bounds the replay window of a captured manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    /// Strict UTC timestamp after which the manifest should not be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    pub resources: Vec<ResourceDoc>,
    /// Typed metadata edges between resources. No runtime semantics in v0.1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignatureBlock>,
}

/// One resource in a manifest (spec §5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDoc {
    /// Stable identifier within the manifest: `[a-z0-9][a-z0-9_-]{0,63}`.
    pub id: String,
    /// Resource kind (see [`Kind`]).
    pub kind: String,
    /// Kind-specific target: URL, absolute path, or nested mURL.
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Semantic role (`source`, `docs`, `issues`, ...). Free-form vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// When true, failure of this resource fails the whole activation.
    #[serde(default)]
    pub required: bool,
    /// Launch-ordering weight; lower dispatches earlier. Default 100.
    #[serde(default = "default_order")]
    pub order: i64,
    /// Ids of resources that must be dispatched before this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `sha256-<base64>` pin over the raw bytes of a nested manifest.
    /// Only meaningful for `kind: murl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// Free-form metadata. Opaque to the resolver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

fn default_order() -> i64 {
    100
}

/// A typed metadata edge between two resources (spec §5.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub from: String,
    pub rel: String,
    pub to: String,
}

/// Detached signature block (spec §7). The signature covers the MCF-1
/// canonical form of the manifest with this member removed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureBlock {
    /// Always `"ed25519"` in v0.1.
    pub alg: String,
    /// `ed25519:<first 16 hex of sha256(publicKey)>`.
    pub key_id: String,
    /// Base64 (standard, padded) of the 32-byte public key.
    pub public_key: String,
    /// Base64 (standard, padded) of the 64-byte signature.
    pub sig: String,
}

/// One validation finding, addressed by a JSON-pointer-ish path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

/// The result of validating a manifest. Errors are spec violations;
/// warnings are permitted-but-suspicious constructs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationReport {
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.errors.push(Issue {
            path: path.into(),
            message: message.into(),
        });
    }

    fn warn(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.warnings.push(Issue {
            path: path.into(),
            message: message.into(),
        });
    }

    /// Collapse into a `Result`, keeping the first few errors in the message.
    pub fn into_result(self) -> Result<ValidationReport> {
        if self.is_valid() {
            Ok(self)
        } else {
            let mut parts: Vec<String> = self
                .errors
                .iter()
                .take(5)
                .map(|i| format!("{}: {}", i.path, i.message))
                .collect();
            if self.errors.len() > 5 {
                parts.push(format!("... and {} more", self.errors.len() - 5));
            }
            Err(Error::Validation(parts.join("; ")))
        }
    }
}

impl Manifest {
    /// Parse manifest bytes. Enforces the size cap *before* parsing and
    /// requires a top-level JSON object.
    pub fn from_slice(bytes: &[u8], limits: &Limits) -> Result<Manifest> {
        if bytes.len() > limits.max_manifest_bytes {
            return Err(Error::LimitExceeded(format!(
                "manifest is {} bytes, limit is {}",
                bytes.len(),
                limits.max_manifest_bytes
            )));
        }
        // Strict parse: duplicate object members are invalid (spec §5.1,
        // threat T-15) and fail here, before a Value exists.
        let raw: Value = crate::json::from_slice_strict(bytes)
            .map_err(|e| Error::Manifest(format!("invalid JSON: {e}")))?;
        if !raw.is_object() {
            return Err(Error::Manifest(
                "top-level JSON value must be an object".into(),
            ));
        }
        let doc: ManifestDoc = serde_json::from_value(raw.clone())
            .map_err(|e| Error::Manifest(format!("schema mismatch: {e}")))?;
        Ok(Manifest { raw, doc })
    }

    /// Validate against the v0.1 specification. Returns findings rather than
    /// failing fast so authors see every problem in one pass.
    pub fn validate(&self) -> ValidationReport {
        let mut rep = ValidationReport::default();
        let d = &self.doc;

        if !ACCEPTED_MURL_VERSIONS.contains(&d.murl_version.as_str()) {
            rep.error(
                "/murlVersion",
                format!(
                    "unsupported version `{}` (this implementation accepts {})",
                    d.murl_version,
                    ACCEPTED_MURL_VERSIONS.join(", ")
                ),
            );
        }

        if d.name.is_empty() || d.name.chars().count() > MAX_NAME_LEN {
            rep.error("/name", format!("must be 1..={MAX_NAME_LEN} characters"));
        }
        if d.name.chars().any(char::is_control) {
            rep.error("/name", "must not contain control characters");
        }
        if let Some(desc) = &d.description {
            if desc.chars().count() > MAX_DESCRIPTION_LEN {
                rep.error(
                    "/description",
                    format!("must be <= {MAX_DESCRIPTION_LEN} characters"),
                );
            }
            if desc.chars().any(char::is_control) {
                rep.error("/description", "must not contain control characters");
            }
        }

        if let Some(id) = &d.id {
            match Murl::parse(id) {
                Ok(m) => {
                    if m.selector.is_some() || m.query.is_some() {
                        rep.error("/id", "must not carry a selector or query");
                    }
                }
                Err(e) => rep.error("/id", format!("not a valid mURL: {e}")),
            }
        }

        if let Some(v) = &d.version {
            match crate::murl::VersionTag::parse(v) {
                Ok(crate::murl::VersionTag::Pinned(_)) => {}
                Ok(crate::murl::VersionTag::Latest) => {
                    rep.error(
                        "/version",
                        "manifest version must be concrete, not `latest`",
                    );
                }
                Err(e) => rep.error("/version", e.to_string()),
            }
        }

        if let Some(exp) = &d.expires {
            if let Err(e) = parse_rfc3339_utc(exp) {
                rep.error("/expires", e);
            }
        }
        if let Some(nb) = &d.not_before {
            if let Err(e) = parse_rfc3339_utc(nb) {
                rep.error("/notBefore", e);
            }
        }
        if let (Some(nb), Some(exp)) = (&d.not_before, &d.expires) {
            if let (Ok(nb_t), Ok(exp_t)) = (parse_rfc3339_utc(nb), parse_rfc3339_utc(exp)) {
                if nb_t >= exp_t {
                    rep.error("/notBefore", "must be strictly before /expires");
                }
            }
        }

        self.validate_resources(&mut rep);
        self.validate_relations(&mut rep);
        self.validate_signature_shape(&mut rep);
        self.validate_unknown_members(&mut rep);
        self.validate_numbers(&mut rep);
        rep
    }

    /// Spec §5.1: every number in a manifest MUST be an integer within
    /// i64/u64. Floats anywhere — including inside `meta` — would fall
    /// outside MCF-1 and make the document unsignable; they are format
    /// errors, not quirks.
    fn validate_numbers(&self, rep: &mut ValidationReport) {
        fn walk(value: &Value, path: &str, rep: &mut ValidationReport) {
            match value {
                Value::Number(n) => {
                    if n.as_i64().is_none() && n.as_u64().is_none() {
                        rep.error(path.to_owned(), format!("non-integer number `{n}`"));
                    }
                }
                Value::Array(items) => {
                    for (i, item) in items.iter().enumerate() {
                        walk(item, &format!("{path}/{i}"), rep);
                    }
                }
                Value::Object(map) => {
                    for (k, v) in map {
                        walk(v, &format!("{path}/{k}"), rep);
                    }
                }
                _ => {}
            }
        }
        walk(&self.raw, "", rep);
    }

    fn validate_resources(&self, rep: &mut ValidationReport) {
        let resources = &self.doc.resources;
        if resources.is_empty() {
            rep.error("/resources", "manifest must declare at least one resource");
            return;
        }
        if resources.len() > MAX_RESOURCES_PER_MANIFEST {
            rep.error(
                "/resources",
                format!(
                    "{} resources exceeds the limit of {MAX_RESOURCES_PER_MANIFEST}",
                    resources.len()
                ),
            );
        }

        let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut seen_targets: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();

        for (i, r) in resources.iter().enumerate() {
            let p = |field: &str| format!("/resources/{i}/{field}");

            if !is_valid_resource_id(&r.id) {
                rep.error(
                    p("id"),
                    format!("`{}` must match [a-z0-9][a-z0-9_-]{{0,63}}", r.id),
                );
            }
            if !seen_ids.insert(r.id.as_str()) {
                rep.error(p("id"), format!("duplicate resource id `{}`", r.id));
            }

            let kind = match Kind::parse(&r.kind) {
                Ok(k) => Some(k),
                Err(e) => {
                    rep.error(p("kind"), e);
                    None
                }
            };

            if r.target.is_empty() || r.target.len() > MAX_TARGET_LEN {
                rep.error(p("target"), format!("must be 1..={MAX_TARGET_LEN} bytes"));
            } else if r.target.chars().any(char::is_control) {
                rep.error(p("target"), "must not contain control characters");
            } else if let Some(kind) = &kind {
                if let Err(e) = validate_target(kind, &r.target) {
                    rep.error(p("target"), e);
                }
                if !seen_targets.insert((kind.to_string(), r.target.clone())) {
                    rep.warn(
                        p("target"),
                        "duplicate (kind, target) pair; duplicates are skipped at dispatch",
                    );
                }
                if r.integrity.is_some() && !matches!(kind, Kind::Murl) {
                    rep.warn(
                        p("integrity"),
                        "integrity pins are only enforced for kind `murl`",
                    );
                }
            }

            if let Some(label) = &r.label {
                if label.is_empty() || label.chars().count() > MAX_LABEL_LEN {
                    rep.error(
                        p("label"),
                        format!("must be 1..={MAX_LABEL_LEN} characters"),
                    );
                }
                if label.chars().any(char::is_control) {
                    rep.error(p("label"), "must not contain control characters");
                }
            }
            if let Some(role) = &r.role {
                if !is_valid_role(role) {
                    rep.error(
                        p("role"),
                        format!("`{role}` must match [a-z0-9][a-z0-9-]{{0,31}}"),
                    );
                }
            }
            if !(0..=10_000).contains(&r.order) {
                rep.error(p("order"), "must be in 0..=10000");
            }
            if r.tags.len() > MAX_TAGS {
                rep.error(p("tags"), format!("at most {MAX_TAGS} tags"));
            }
            for t in &r.tags {
                if !is_valid_tag(t) {
                    rep.error(
                        p("tags"),
                        format!("tag `{t}` must match [a-z0-9-]{{1,{MAX_TAG_LEN}}}"),
                    );
                }
            }
            if r.depends_on.len() > MAX_DEPENDS {
                rep.error(
                    p("dependsOn"),
                    format!("at most {MAX_DEPENDS} dependencies"),
                );
            }
            for dep in &r.depends_on {
                if dep == &r.id {
                    rep.error(p("dependsOn"), format!("`{}` depends on itself", r.id));
                } else if !resources.iter().any(|other| &other.id == dep) {
                    rep.error(p("dependsOn"), format!("unknown resource id `{dep}`"));
                }
            }
            if let Some(integrity) = &r.integrity {
                if !is_valid_integrity(integrity) {
                    rep.error(
                        p("integrity"),
                        "must be `sha256-<44 base64 chars>` over the nested manifest bytes",
                    );
                }
            }
        }

        // dependsOn cycles (only meaningful once ids resolved).
        if rep.errors.is_empty() {
            if let Err(e) = crate::graph::execution_order(resources) {
                rep.error("/resources", e);
            }
        }
    }

    fn validate_relations(&self, rep: &mut ValidationReport) {
        let relations = &self.doc.relations;
        if relations.len() > MAX_RELATIONS {
            rep.error("/relations", format!("at most {MAX_RELATIONS} relations"));
        }
        for (i, rel) in relations.iter().enumerate() {
            let p = format!("/relations/{i}");
            for (field, id) in [("from", &rel.from), ("to", &rel.to)] {
                if !self.doc.resources.iter().any(|r| &r.id == id) {
                    rep.error(
                        format!("{p}/{field}"),
                        format!("unknown resource id `{id}`"),
                    );
                }
            }
            if rel.rel.is_empty()
                || rel.rel.len() > 32
                || !rel.rel.bytes().enumerate().all(|(j, b)| {
                    if j == 0 {
                        b.is_ascii_lowercase()
                    } else {
                        b.is_ascii_lowercase() || b == b'-'
                    }
                })
            {
                rep.error(
                    format!("{p}/rel"),
                    format!("`{}` must match [a-z][a-z-]{{0,31}}", rel.rel),
                );
            }
        }
    }

    fn validate_signature_shape(&self, rep: &mut ValidationReport) {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;
        let Some(sig) = &self.doc.signature else {
            return;
        };
        if sig.alg != "ed25519" {
            rep.error(
                "/signature/alg",
                format!("unsupported algorithm `{}`", sig.alg),
            );
        }
        match b64.decode(&sig.public_key) {
            Ok(pk) if pk.len() == 32 => {}
            _ => rep.error("/signature/publicKey", "must be base64 of 32 bytes"),
        }
        match b64.decode(&sig.sig) {
            Ok(s) if s.len() == 64 => {}
            _ => rep.error("/signature/sig", "must be base64 of 64 bytes"),
        }
        let kid_ok = sig.key_id.strip_prefix("ed25519:").is_some_and(|hex| {
            hex.len() == 16
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        });
        if !kid_ok {
            rep.error(
                "/signature/keyId",
                "must be `ed25519:<16 lowercase hex chars>`",
            );
        }
    }

    /// Unknown members are warnings, not errors: future minor spec versions
    /// may add members, and signatures still cover them via the raw document.
    fn validate_unknown_members(&self, rep: &mut ValidationReport) {
        const TOP: &[&str] = &[
            "murlVersion",
            "id",
            "name",
            "description",
            "version",
            "notBefore",
            "expires",
            "resources",
            "relations",
            "signature",
        ];
        const RESOURCE: &[&str] = &[
            "id",
            "kind",
            "target",
            "label",
            "role",
            "required",
            "order",
            "dependsOn",
            "tags",
            "integrity",
            "meta",
        ];
        const RELATION: &[&str] = &["from", "rel", "to"];
        const SIGNATURE: &[&str] = &["alg", "keyId", "publicKey", "sig"];

        let Some(obj) = self.raw.as_object() else {
            return;
        };
        for k in obj.keys() {
            if !TOP.contains(&k.as_str()) {
                rep.warn(
                    format!("/{k}"),
                    "unknown member (ignored by this implementation)",
                );
            }
        }
        if let Some(resources) = obj.get("resources").and_then(Value::as_array) {
            for (i, r) in resources.iter().enumerate() {
                if let Some(robj) = r.as_object() {
                    for k in robj.keys() {
                        if !RESOURCE.contains(&k.as_str()) {
                            rep.warn(format!("/resources/{i}/{k}"), "unknown member (ignored)");
                        }
                    }
                }
            }
        }
        if let Some(relations) = obj.get("relations").and_then(Value::as_array) {
            for (i, r) in relations.iter().enumerate() {
                if let Some(robj) = r.as_object() {
                    for k in robj.keys() {
                        if !RELATION.contains(&k.as_str()) {
                            rep.warn(format!("/relations/{i}/{k}"), "unknown member (ignored)");
                        }
                    }
                }
            }
        }
        if let Some(sobj) = obj.get("signature").and_then(Value::as_object) {
            for k in sobj.keys() {
                if !SIGNATURE.contains(&k.as_str()) {
                    rep.warn(format!("/signature/{k}"), "unknown member (ignored)");
                }
            }
        }
    }
}

// Identifier grammars are shared with the selector parser (crate::grammar)
// so the two can never drift.
pub use crate::grammar::is_valid_resource_id;
use crate::grammar::{is_valid_role, is_valid_tag};

fn is_valid_integrity(s: &str) -> bool {
    s.strip_prefix("sha256-").is_some_and(|b64| {
        b64.len() == 44
            && b64
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    })
}

/// Per-kind target validation (spec §5.2.3).
pub fn validate_target(kind: &Kind, target: &str) -> std::result::Result<(), String> {
    match kind {
        Kind::Https => validate_web_target(target),
        Kind::File | Kind::Dir | Kind::Terminal => validate_path_target(target),
        Kind::Murl => match Murl::parse(target) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("not a valid mURL: {e}")),
        },
        Kind::Ssh => validate_ssh_target(target),
        Kind::RemoteDesktop => validate_remote_desktop_target(target),
        Kind::Geo => validate_geo_target(target),
        Kind::Mailto => validate_mailto_target(target),
        Kind::Custom(_) => Ok(()), // charset/length already enforced generically
    }
}

/// `ssh://[user@]host[:port][/path]`.
///
/// Userinfo is *permitted* here and nowhere else in mURL: an ssh target
/// without a username is often unusable, and unlike a web URL there is no
/// address-bar spoofing to inherit — the shown target is the shown target,
/// and connecting is DANGEROUS-tier regardless. Everything else stays
/// strict: one `@` at most, no shell metacharacters, no options smuggled
/// through the authority (a leading `-` in a host would become an argument
/// to `ssh` on some handlers).
fn validate_ssh_target(target: &str) -> std::result::Result<(), String> {
    let rest = target
        .strip_prefix("ssh://")
        .ok_or("must start with ssh://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return Err("empty host".into());
    }
    let (user, hostport) = match authority.split_once('@') {
        Some((u, h)) => {
            if h.contains('@') {
                return Err("at most one `@` is allowed".into());
            }
            (Some(u), h)
        }
        None => (None, authority),
    };
    if let Some(user) = user {
        if user.is_empty() || user.len() > 64 {
            return Err("username must be 1..=64 characters".into());
        }
        if !user
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err("username must match [A-Za-z0-9._-]+".into());
        }
        if user.starts_with('-') {
            return Err("username must not start with `-` (it would look like an option)".into());
        }
    }
    validate_hostport(hostport)
}

/// `rdp://host[:port]` or `vnc://host[:port][/...]`.
fn validate_remote_desktop_target(target: &str) -> std::result::Result<(), String> {
    let rest = target
        .strip_prefix("rdp://")
        .or_else(|| target.strip_prefix("vnc://"))
        .ok_or("must start with rdp:// or vnc://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err("userinfo is not allowed in remote-desktop targets".into());
    }
    validate_hostport(authority)
}

fn validate_hostport(hostport: &str) -> std::result::Result<(), String> {
    if hostport.is_empty() {
        return Err("empty host".into());
    }
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (hostport, None),
    };
    if let Some(port) = port {
        let ok = !port.is_empty()
            && port.len() <= 5
            && port.bytes().all(|b| b.is_ascii_digit())
            && (1..=65535).contains(&port.parse::<u32>().unwrap_or(0));
        if !ok {
            return Err(format!("invalid port `{port}`"));
        }
    }
    if host.is_empty() || host.len() > 253 {
        return Err("host must be 1..=253 characters".into());
    }
    if host.starts_with('-') {
        return Err("host must not start with `-` (it would look like an option)".into());
    }
    if !host
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
    {
        return Err("host must match [A-Za-z0-9.-]+ (IDN hosts must be punycoded)".into());
    }
    Ok(())
}

/// `geo:lat,lon[,alt][;u=unc][;<param>]` per RFC 5870.
fn validate_geo_target(target: &str) -> std::result::Result<(), String> {
    let rest = target.strip_prefix("geo:").ok_or("must start with geo:")?;
    let mut parts = rest.split(';');
    let coords = parts.next().unwrap_or("");
    let numbers: Vec<&str> = coords.split(',').collect();
    if !(2..=3).contains(&numbers.len()) {
        return Err("expected `lat,lon` or `lat,lon,alt`".into());
    }
    for (i, n) in numbers.iter().enumerate() {
        let value: f64 = n
            .parse()
            .map_err(|_| format!("coordinate `{n}` is not a number"))?;
        if !value.is_finite() {
            return Err(format!("coordinate `{n}` is not finite"));
        }
        match i {
            0 if !(-90.0..=90.0).contains(&value) => {
                return Err(format!("latitude {value} is outside -90..=90"))
            }
            1 if !(-180.0..=180.0).contains(&value) => {
                return Err(format!("longitude {value} is outside -180..=180"))
            }
            _ => {}
        }
    }
    for param in parts {
        if param.is_empty() {
            return Err("empty `;` parameter".into());
        }
        if !param
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'=' | b'.' | b'-' | b'_' | b'%'))
        {
            return Err(format!("parameter `{param}` has invalid characters"));
        }
    }
    Ok(())
}

/// `mailto:addr[,addr...][?headers]` per RFC 6068.
///
/// Header names are restricted to a safe list: a manifest may pre-fill a
/// subject or body, but must not be able to smuggle a `bcc` a user won't
/// notice, or headers a client might act on beyond composing.
fn validate_mailto_target(target: &str) -> std::result::Result<(), String> {
    const ALLOWED_HEADERS: &[&str] = &["subject", "body", "cc", "to"];

    let rest = target
        .strip_prefix("mailto:")
        .ok_or("must start with mailto:")?;
    let (addresses, query) = match rest.split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (rest, None),
    };
    if addresses.is_empty() {
        return Err("at least one address is required".into());
    }
    for address in addresses.split(',') {
        let (local, domain) = address
            .split_once('@')
            .ok_or_else(|| format!("`{address}` is not an addr-spec"))?;
        if local.is_empty() || domain.is_empty() || domain.contains('@') {
            return Err(format!("`{address}` is not an addr-spec"));
        }
        if !domain
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        {
            return Err(format!("domain in `{address}` has invalid characters"));
        }
    }
    if let Some(query) = query {
        for pair in query.split('&') {
            let (name, _value) = pair
                .split_once('=')
                .ok_or_else(|| format!("header `{pair}` must be name=value"))?;
            let lower = name.to_ascii_lowercase();
            if !ALLOWED_HEADERS.contains(&lower.as_str()) {
                return Err(format!(
                    "header `{name}` is not allowed (permitted: {})",
                    ALLOWED_HEADERS.join(", ")
                ));
            }
        }
    }
    Ok(())
}

fn validate_web_target(target: &str) -> std::result::Result<(), String> {
    let (rest, plain_http) = if let Some(r) = target.strip_prefix("https://") {
        (r, false)
    } else if let Some(r) = target.strip_prefix("http://") {
        (r, true)
    } else {
        return Err("must start with https:// (http:// is allowed for loopback hosts only)".into());
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return Err("empty host".into());
    }
    if authority.contains('@') {
        return Err("userinfo in web targets is forbidden (phishing vector)".into());
    }
    let host = authority
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if host.is_empty() {
        return Err("empty host".into());
    }
    if target.contains(' ') {
        return Err("must not contain spaces (percent-encode them)".into());
    }
    if plain_http {
        let loopback = host == "localhost" || host == "127.0.0.1" || host.ends_with(".localhost");
        if !loopback {
            return Err(format!(
                "plain http:// is only allowed for loopback hosts, not `{host}`"
            ));
        }
    }
    Ok(())
}

/// Paths must be absolute (or `~`-rooted) and free of dot segments. There is
/// no "relative to what?" answer that survives OS-handler activation, and
/// `..` in a manifest is a traversal attempt, not a convenience.
fn validate_path_target(target: &str) -> std::result::Result<(), String> {
    let body = if target == "~" {
        return Ok(());
    } else if let Some(rest) = target.strip_prefix("~/") {
        rest
    } else if let Some(rest) = target.strip_prefix('/') {
        rest
    } else {
        let b = target.as_bytes();
        let windows_abs = b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'/' || b[2] == b'\\');
        if windows_abs {
            &target[3..]
        } else {
            return Err("must be an absolute path (or start with ~/)".into());
        }
    };
    for seg in body.split(['/', '\\']) {
        if seg == ".." || seg == "." {
            return Err("dot segments are forbidden in path targets".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Manifest {
        Manifest::from_slice(json.as_bytes(), &Limits::default()).unwrap()
    }

    fn minimal(resources: &str) -> String {
        format!(r#"{{"murlVersion":"0.1","name":"Test","resources":[{resources}]}}"#)
    }

    const RES_OK: &str = r#"{"id":"src","kind":"https","target":"https://example.com/x"}"#;

    #[test]
    fn valid_minimal_manifest() {
        let m = parse(&minimal(RES_OK));
        let rep = m.validate();
        assert!(rep.is_valid(), "{:?}", rep.errors);
        assert!(rep.warnings.is_empty());
    }

    #[test]
    fn rejects_wrong_spec_version() {
        let m = parse(&minimal(RES_OK).replace("0.1", "9.9"));
        assert!(!m.validate().is_valid());
    }

    #[test]
    fn rejects_empty_resources() {
        let m = parse(r#"{"murlVersion":"0.1","name":"T","resources":[]}"#);
        assert!(!m.validate().is_valid());
    }

    #[test]
    fn rejects_bad_resource_ids_and_duplicates() {
        let m = parse(&minimal(
            r#"{"id":"BAD","kind":"https","target":"https://e.com"},
               {"id":"x","kind":"https","target":"https://e.com/1"},
               {"id":"x","kind":"https","target":"https://e.com/2"}"#,
        ));
        let rep = m.validate();
        assert_eq!(rep.errors.len(), 2, "{:?}", rep.errors);
    }

    #[test]
    fn rejects_http_for_non_loopback() {
        let m = parse(&minimal(
            r#"{"id":"a","kind":"https","target":"http://example.com/x"}"#,
        ));
        assert!(!m.validate().is_valid());
        let m = parse(&minimal(
            r#"{"id":"a","kind":"https","target":"http://localhost:8080/x"}"#,
        ));
        assert!(m.validate().is_valid());
    }

    #[test]
    fn rejects_userinfo_in_web_targets() {
        let m = parse(&minimal(
            r#"{"id":"a","kind":"https","target":"https://github.com@evil.example/x"}"#,
        ));
        assert!(!m.validate().is_valid());
    }

    #[test]
    fn rejects_relative_and_traversal_paths() {
        for target in [
            "relative/path",
            "../etc/passwd",
            "/home/u/../../etc/shadow",
            "~/x/../y",
        ] {
            let m = parse(&minimal(&format!(
                r#"{{"id":"a","kind":"file","target":"{target}"}}"#
            )));
            assert!(!m.validate().is_valid(), "should reject {target}");
        }
        for target in ["/home/u/notes.txt", "~/projects/x", "C:/Users/u/doc.pdf"] {
            let m = parse(&minimal(&format!(
                r#"{{"id":"a","kind":"file","target":"{target}"}}"#
            )));
            assert!(m.validate().is_valid(), "should accept {target}");
        }
    }

    #[test]
    fn rejects_unknown_depends_and_self_depends() {
        let m = parse(&minimal(
            r#"{"id":"a","kind":"https","target":"https://e.com/1","dependsOn":["ghost"]},
               {"id":"b","kind":"https","target":"https://e.com/2","dependsOn":["b"]}"#,
        ));
        let rep = m.validate();
        assert_eq!(rep.errors.len(), 2, "{:?}", rep.errors);
    }

    #[test]
    fn rejects_depends_cycles() {
        let m = parse(&minimal(
            r#"{"id":"a","kind":"https","target":"https://e.com/1","dependsOn":["b"]},
               {"id":"b","kind":"https","target":"https://e.com/2","dependsOn":["a"]}"#,
        ));
        assert!(!m.validate().is_valid());
    }

    #[test]
    fn unknown_members_warn_but_pass() {
        let m = parse(
            r#"{"murlVersion":"0.1","name":"T","futureField":1,
                "resources":[{"id":"a","kind":"https","target":"https://e.com","novel":true}]}"#,
        );
        let rep = m.validate();
        assert!(rep.is_valid());
        assert_eq!(rep.warnings.len(), 2);
    }

    #[test]
    fn size_cap_enforced_before_parse() {
        let huge = format!(
            r#"{{"murlVersion":"0.1","name":"T","description":"{}","resources":[]}}"#,
            "a".repeat(300 * 1024)
        );
        let err = Manifest::from_slice(huge.as_bytes(), &Limits::default()).unwrap_err();
        assert!(matches!(err, Error::LimitExceeded(_)));
    }

    #[test]
    fn rejects_non_object_top_level() {
        assert!(Manifest::from_slice(b"[]", &Limits::default()).is_err());
        assert!(Manifest::from_slice(b"42", &Limits::default()).is_err());
        assert!(Manifest::from_slice(b"not json", &Limits::default()).is_err());
    }

    #[test]
    fn validates_nested_murl_and_integrity() {
        let m = parse(&minimal(
            r#"{"id":"team","kind":"murl","target":"murl://example.com/team",
                "integrity":"sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="}"#,
        ));
        assert!(m.validate().is_valid());
        let m = parse(&minimal(
            r#"{"id":"team","kind":"murl","target":"not-a-murl"}"#,
        ));
        assert!(!m.validate().is_valid());
        let m = parse(&minimal(
            r#"{"id":"team","kind":"murl","target":"murl://example.com/team","integrity":"sha256-short"}"#,
        ));
        assert!(!m.validate().is_valid());
    }

    fn target_ok(kind: &str, target: &str) -> bool {
        let m = parse(&minimal(&format!(
            r#"{{"id":"a","kind":"{kind}","target":"{target}"}}"#
        )));
        m.validate().is_valid()
    }

    #[test]
    fn ssh_targets() {
        assert!(target_ok("ssh", "ssh://host.example"));
        assert!(target_ok("ssh", "ssh://deploy@host.example"));
        assert!(target_ok("ssh", "ssh://deploy@host.example:2222"));
        assert!(target_ok("ssh", "ssh://10.0.0.5"));
        // Option smuggling and shell metacharacters must not survive.
        assert!(!target_ok("ssh", "ssh://-oProxyCommand=evil@host"));
        assert!(!target_ok("ssh", "ssh://user@-host"));
        assert!(!target_ok("ssh", "ssh://a@b@c"));
        assert!(!target_ok("ssh", "ssh://host;rm%20-rf"));
        assert!(!target_ok("ssh", "ssh://"));
        assert!(!target_ok("ssh", "http://host"));
        assert!(!target_ok("ssh", "ssh://host:0"));
        assert!(!target_ok("ssh", "ssh://host:99999"));
    }

    #[test]
    fn remote_desktop_targets() {
        assert!(target_ok("remote-desktop", "rdp://desktop.example"));
        assert!(target_ok("remote-desktop", "vnc://10.0.0.9:5901"));
        assert!(!target_ok("remote-desktop", "rdp://user@host"));
        assert!(!target_ok("remote-desktop", "ssh://host"));
        assert!(!target_ok("remote-desktop", "rdp://-x"));
    }

    #[test]
    fn geo_targets() {
        assert!(target_ok("geo", "geo:48.8584,2.2945"));
        assert!(target_ok("geo", "geo:-33.8568,151.2153,10"));
        assert!(target_ok("geo", "geo:48.85,2.29;u=35"));
        assert!(!target_ok("geo", "geo:91,0"));
        assert!(!target_ok("geo", "geo:0,181"));
        assert!(!target_ok("geo", "geo:notanumber,0"));
        assert!(!target_ok("geo", "geo:48.85"));
        assert!(!target_ok("geo", "https://maps.example/48,2"));
    }

    #[test]
    fn mailto_targets() {
        assert!(target_ok("mailto", "mailto:team@example.com"));
        assert!(target_ok("mailto", "mailto:a@example.com,b@example.com"));
        assert!(target_ok(
            "mailto",
            "mailto:team@example.com?subject=Incident&body=Details"
        ));
        // Headers beyond the safe list are refused: a manifest must not be
        // able to add a bcc the user won't see.
        assert!(!target_ok(
            "mailto",
            "mailto:team@example.com?bcc=attacker@evil.example"
        ));
        assert!(!target_ok("mailto", "mailto:not-an-address"));
        assert!(!target_ok("mailto", "mailto:"));
        assert!(!target_ok("mailto", "mailto:a@b?justaflag"));
    }

    #[test]
    fn rejects_duplicate_members_at_any_level() {
        // Top level.
        let err = Manifest::from_slice(
            br#"{"murlVersion":"0.1","murlVersion":"0.1","name":"T",
                 "resources":[{"id":"a","kind":"https","target":"https://e.com"}]}"#,
            &Limits::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err}");
        // Inside a resource — the T-15 shape.
        let err = Manifest::from_slice(
            br#"{"murlVersion":"0.1","name":"T","resources":[
                 {"id":"a","kind":"https","target":"https://safe.example",
                  "target":"https://evil.example"}]}"#,
            &Limits::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err}");
    }

    #[test]
    fn rejects_non_integer_numbers_anywhere() {
        let m = parse(
            r#"{"murlVersion":"0.1","name":"T",
                "resources":[{"id":"a","kind":"https","target":"https://e.com",
                              "meta":{"ratio":1.5}}]}"#,
        );
        let rep = m.validate();
        assert!(!rep.is_valid());
        assert!(
            rep.errors.iter().any(|e| e.message.contains("non-integer")),
            "{:?}",
            rep.errors
        );
    }

    #[test]
    fn not_before_is_strict_and_ordered() {
        let m = parse(
            r#"{"murlVersion":"0.1","name":"T","notBefore":"2030-01-01T00:00:00Z",
                "expires":"2031-01-01T00:00:00Z",
                "resources":[{"id":"a","kind":"https","target":"https://e.com"}]}"#,
        );
        assert!(m.validate().is_valid());
        // Lenient formats rejected.
        let m = parse(
            r#"{"murlVersion":"0.1","name":"T","notBefore":"2030-01-01",
                "resources":[{"id":"a","kind":"https","target":"https://e.com"}]}"#,
        );
        assert!(!m.validate().is_valid());
        // notBefore >= expires rejected.
        let m = parse(
            r#"{"murlVersion":"0.1","name":"T","notBefore":"2031-01-01T00:00:00Z",
                "expires":"2030-01-01T00:00:00Z",
                "resources":[{"id":"a","kind":"https","target":"https://e.com"}]}"#,
        );
        assert!(!m.validate().is_valid());
    }

    #[test]
    fn expired_format_is_strict() {
        let m = parse(
            r#"{"murlVersion":"0.1","name":"T","expires":"2030-01-01T00:00:00Z",
                "resources":[{"id":"a","kind":"https","target":"https://e.com"}]}"#,
        );
        assert!(m.validate().is_valid());
        let m = parse(
            r#"{"murlVersion":"0.1","name":"T","expires":"2030-01-01",
                "resources":[{"id":"a","kind":"https","target":"https://e.com"}]}"#,
        );
        assert!(!m.validate().is_valid());
    }
}
