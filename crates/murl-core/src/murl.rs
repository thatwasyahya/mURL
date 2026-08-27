//! Parser and model for the `murl:` scheme.
//!
//! Grammar (normative reference: `spec/SPECIFICATION.md` §3):
//!
//! ```text
//! murl      = "murl://" authority "/" name [ "@" version ] [ "?" query ] [ "#" selector ]
//! authority = "local" / host [ ":" port ]
//! name      = segment *( "/" segment )        ; 1..=8 segments
//! version   = "latest" / int *( "." int )     ; 1..=3 dotted integers
//! selector  = item *( "," item )              ; 1..=8 items
//! item      = resource-id / "role=" role / "tag=" tag
//! ```
//!
//! Deliberate deviations from generic RFC 3986 syntax, all security-motivated:
//!
//! * **Userinfo is forbidden.** `murl://github.com@evil.example/x` is a parse
//!   error, not a lookup against `evil.example`. The `user@host` production is
//!   one of the oldest phishing primitives in URL history and mURL has no use
//!   for it (authentication is a property of *resources*, not of the name).
//! * **The entire mURL must be printable ASCII.** Non-ASCII must be
//!   percent-encoded UTF-8. This removes homoglyph tricks from the raw string
//!   and keeps mURLs safe to log, diff, and transport.
//! * **Dot segments (`.`, `..`) are rejected**, including percent-encoded
//!   forms, closing path-traversal against filesystem-backed stores.
//! * **IPv6 literals are not supported in v0.1** (`[::1]` is rejected). IPv4
//!   dotted-quads parse as ordinary reg-names.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hard cap on the length of a raw mURL string.
pub const MAX_MURL_LEN: usize = 1024;
/// Maximum number of name segments.
pub const MAX_SEGMENTS: usize = 8;
/// Maximum decoded byte length of a single name segment.
pub const MAX_SEGMENT_BYTES: usize = 64;
/// Maximum authority (host) length, matching DNS.
pub const MAX_AUTHORITY_LEN: usize = 253;
/// Maximum length of one selector item's value.
pub const MAX_SELECTOR_LEN: usize = 64;
/// Maximum number of comma-separated selector items.
pub const MAX_SELECTOR_ITEMS: usize = 8;

/// The reserved authority that resolves against the local name store instead
/// of the network.
pub const LOCAL_AUTHORITY: &str = "local";

/// Errors produced while parsing an mURL string.
///
/// Every variant corresponds to a MUST-level requirement in the specification;
/// none of them are recoverable by "guessing what the user meant". A parser
/// that repairs malformed identifiers is a parser that can be steered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MurlParseError {
    #[error("mURL exceeds maximum length of {MAX_MURL_LEN} bytes")]
    TooLong,
    #[error("mURL contains a non-printable or non-ASCII byte at offset {0}; non-ASCII must be percent-encoded")]
    InvalidCharacter(usize),
    #[error("mURL must start with the scheme `murl://`")]
    MissingScheme,
    #[error("mURL has an empty authority")]
    MissingAuthority,
    #[error("userinfo (`user@host`) is forbidden in mURL authorities")]
    UserinfoForbidden,
    #[error("IPv6 literals are not supported in mURL v0.1")]
    Ipv6NotSupported,
    #[error("invalid authority: {0}")]
    InvalidAuthority(String),
    #[error("invalid port: {0}")]
    InvalidPort(String),
    #[error("mURL is missing a name (expected `murl://authority/name`)")]
    MissingName,
    #[error("name contains an empty segment")]
    EmptySegment,
    #[error("name has more than {MAX_SEGMENTS} segments")]
    TooManySegments,
    #[error("name segment exceeds {MAX_SEGMENT_BYTES} bytes after decoding")]
    SegmentTooLong,
    #[error("invalid name segment: {0}")]
    InvalidSegment(String),
    #[error("invalid percent-encoding: {0}")]
    InvalidPercentEncoding(String),
    #[error("`@` may appear only once, as a version marker on the final name segment")]
    MisplacedVersionMarker,
    #[error("invalid version tag: {0}")]
    InvalidVersion(String),
    #[error("invalid selector: {0}")]
    InvalidSelector(String),
    #[error("invalid query: {0}")]
    InvalidQuery(String),
}

/// The authority component of an mURL: who controls the namespace the name
/// lives in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Authority {
    /// The reserved `local` authority. Names resolve against the local,
    /// user-controlled name store and never touch the network.
    Local,
    /// A DNS-based authority. Names resolve over HTTPS via the authority's
    /// `/.well-known/murl/` endpoint. Ownership of the namespace is ownership
    /// of the DNS name — mURL introduces no new registry.
    Remote {
        /// Lowercased reg-name (DNS host or IPv4 literal).
        host: String,
        /// Optional explicit port. Primarily useful for loopback development.
        port: Option<u16>,
    },
}

impl Authority {
    /// True when this authority resolves without any network access.
    pub fn is_local(&self) -> bool {
        matches!(self, Authority::Local)
    }

    /// True for loopback remote authorities (`localhost`, `127.0.0.1`),
    /// which are permitted to resolve over plain HTTP for development.
    pub fn is_loopback(&self) -> bool {
        match self {
            Authority::Local => false,
            Authority::Remote { host, .. } => {
                host == "localhost" || host == "127.0.0.1" || host.ends_with(".localhost")
            }
        }
    }
}

impl fmt::Display for Authority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Authority::Local => f.write_str(LOCAL_AUTHORITY),
            Authority::Remote { host, port } => match port {
                Some(p) => write!(f, "{host}:{p}"),
                None => f.write_str(host),
            },
        }
    }
}

/// The version tag of an mURL.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VersionTag {
    /// The default: resolve to whatever the authority currently publishes.
    Latest,
    /// A pinned version (`@1`, `@1.4`, `@1.4.2`). Pinned resolutions are
    /// expected to be immutable and are eligible for indefinite caching.
    Pinned(Vec<u32>),
}

impl VersionTag {
    /// Parse the text after the `@` marker.
    pub fn parse(s: &str) -> Result<Self, MurlParseError> {
        if s == "latest" {
            return Ok(VersionTag::Latest);
        }
        if s.is_empty() {
            return Err(MurlParseError::InvalidVersion("empty version".into()));
        }
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() > 3 {
            return Err(MurlParseError::InvalidVersion(format!(
                "`{s}`: at most 3 dotted components"
            )));
        }
        let mut nums = Vec::with_capacity(parts.len());
        for p in parts {
            if p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(MurlParseError::InvalidVersion(format!(
                    "`{s}`: components must be 1-5 digit integers"
                )));
            }
            if p.len() > 1 && p.starts_with('0') {
                return Err(MurlParseError::InvalidVersion(format!(
                    "`{s}`: no leading zeros"
                )));
            }
            // len <= 5 guarantees the parse cannot overflow u32.
            nums.push(p.parse::<u32>().expect("digits within u32 range"));
        }
        Ok(VersionTag::Pinned(nums))
    }

    pub fn is_latest(&self) -> bool {
        matches!(self, VersionTag::Latest)
    }
}

impl fmt::Display for VersionTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionTag::Latest => f.write_str("latest"),
            VersionTag::Pinned(v) => {
                let s: Vec<String> = v.iter().map(u32::to_string).collect();
                f.write_str(&s.join("."))
            }
        }
    }
}

/// One item of a `#selector` fragment (spec §6.7).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SelectorItem {
    /// Matches the root-manifest resource with this id (and, for a `murl`
    /// container, its spliced children).
    Id(String),
    /// `role=<role>` — matches every planned resource with this role.
    Role(String),
    /// `tag=<tag>` — matches every planned resource carrying this tag.
    Tag(String),
}

impl fmt::Display for SelectorItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectorItem::Id(s) => f.write_str(s),
            SelectorItem::Role(s) => write!(f, "role={s}"),
            SelectorItem::Tag(s) => write!(f, "tag={s}"),
        }
    }
}

/// A parsed, validated mURL.
///
/// Invariants (enforced by [`Murl::parse`], relied upon everywhere else):
/// name segments are non-empty decoded UTF-8 with no control characters, no
/// path separators, and are not dot segments; the authority is a lowercase
/// reg-name or `local`; selector items, when present, satisfy the shared
/// id/role/tag grammars.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Murl {
    pub authority: Authority,
    /// Decoded name segments (at least one).
    pub name: Vec<String>,
    pub version: VersionTag,
    /// Optional `#selector` items addressing a subset of the destination.
    pub selector: Option<Vec<SelectorItem>>,
    /// Raw query string, preserved but assigned no semantics in v0.1.
    pub query: Option<String>,
}

impl Murl {
    /// Parse an mURL string. This is a total function over arbitrary input:
    /// it either returns a fully-validated [`Murl`] or a precise error, and
    /// never panics (fuzz target: `fuzz_targets/parse_murl.rs`).
    pub fn parse(input: &str) -> Result<Self, MurlParseError> {
        if input.len() > MAX_MURL_LEN {
            return Err(MurlParseError::TooLong);
        }
        for (i, b) in input.bytes().enumerate() {
            // Printable ASCII only. Space is not legal in any URI.
            if !(0x21..=0x7e).contains(&b) {
                return Err(MurlParseError::InvalidCharacter(i));
            }
        }

        // Scheme, case-insensitive per RFC 3986 §3.1.
        let rest = strip_scheme(input).ok_or(MurlParseError::MissingScheme)?;

        // Fragment first (a `#` ends everything else), then query.
        let (rest, fragment) = match rest.split_once('#') {
            Some((r, f)) => (r, Some(f)),
            None => (rest, None),
        };
        let (rest, query) = match rest.split_once('?') {
            Some((r, q)) => (r, Some(q)),
            None => (rest, None),
        };

        // Authority / path split.
        let (auth_raw, path_raw) = rest.split_once('/').ok_or(MurlParseError::MissingName)?;
        let authority = parse_authority(auth_raw)?;

        if path_raw.is_empty() {
            return Err(MurlParseError::MissingName);
        }

        // Split segments; the version marker may only decorate the final one.
        let raw_segments: Vec<&str> = path_raw.split('/').collect();
        if raw_segments.len() > MAX_SEGMENTS {
            return Err(MurlParseError::TooManySegments);
        }
        let mut segments: Vec<String> = Vec::with_capacity(raw_segments.len());
        let mut version = VersionTag::Latest;
        let last_idx = raw_segments.len() - 1;
        for (i, raw) in raw_segments.iter().enumerate() {
            let mut seg: &str = raw;
            if i == last_idx {
                if let Some((base, ver)) = raw.split_once('@') {
                    if ver.contains('@') {
                        return Err(MurlParseError::MisplacedVersionMarker);
                    }
                    if base.is_empty() {
                        return Err(MurlParseError::EmptySegment);
                    }
                    version = VersionTag::parse(ver)?;
                    seg = base;
                }
            } else if raw.contains('@') {
                return Err(MurlParseError::MisplacedVersionMarker);
            }
            segments.push(decode_segment(seg)?);
        }

        let selector = match fragment {
            None => None,
            Some(f) => Some(parse_selector(f)?),
        };

        if let Some(q) = query {
            // No semantics in v0.1, but still constrained to sane characters
            // so a query cannot smuggle a second authority or fragment.
            if q.contains('#') || q.len() > 512 {
                return Err(MurlParseError::InvalidQuery(q.into()));
            }
        }

        Ok(Murl {
            authority,
            name: segments,
            version,
            selector,
            query: query.map(str::to_owned),
        })
    }

    /// The canonical resolution identity of this mURL: authority + name +
    /// version, with selector and query stripped. Used as the cache key and
    /// for cycle detection during recursive resolution.
    pub fn identity(&self) -> String {
        let mut s = format!("murl://{}", self.authority);
        for seg in &self.name {
            s.push('/');
            s.push_str(&encode_segment(seg));
        }
        if let VersionTag::Pinned(_) = self.version {
            s.push('@');
            s.push_str(&self.version.to_string());
        }
        s
    }

    /// The final name segment (the "short name").
    pub fn short_name(&self) -> &str {
        self.name.last().expect("parser guarantees >= 1 segment")
    }

    /// The name path joined with `/`, undecorated.
    pub fn name_path(&self) -> String {
        self.name.join("/")
    }

    /// The selector rendered back to its fragment form (without `#`).
    pub fn selector_display(&self) -> Option<String> {
        self.selector.as_ref().map(|items| {
            items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        })
    }
}

impl fmt::Display for Murl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.identity())?;
        if let Some(q) = &self.query {
            write!(f, "?{q}")?;
        }
        if let Some(sel) = self.selector_display() {
            write!(f, "#{sel}")?;
        }
        Ok(())
    }
}

fn strip_scheme(input: &str) -> Option<&str> {
    if input.len() < 7 {
        return None;
    }
    let (scheme, rest) = input.split_at(7);
    if scheme.eq_ignore_ascii_case("murl://") {
        Some(rest)
    } else {
        None
    }
}

fn parse_authority(raw: &str) -> Result<Authority, MurlParseError> {
    if raw.is_empty() {
        return Err(MurlParseError::MissingAuthority);
    }
    if raw.contains('@') {
        return Err(MurlParseError::UserinfoForbidden);
    }
    if raw.contains('[') || raw.contains(']') {
        return Err(MurlParseError::Ipv6NotSupported);
    }
    let (host_raw, port) = match raw.split_once(':') {
        Some((h, p)) => {
            if p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(MurlParseError::InvalidPort(p.into()));
            }
            let port: u32 = p.parse().expect("digits");
            if port == 0 || port > 65535 {
                return Err(MurlParseError::InvalidPort(p.into()));
            }
            (h, Some(port as u16))
        }
        None => (raw, None),
    };
    let host = host_raw.to_ascii_lowercase();
    if host.len() > MAX_AUTHORITY_LEN {
        return Err(MurlParseError::InvalidAuthority("host too long".into()));
    }
    if host == LOCAL_AUTHORITY {
        if port.is_some() {
            return Err(MurlParseError::InvalidAuthority(
                "the `local` authority takes no port".into(),
            ));
        }
        return Ok(Authority::Local);
    }
    // reg-name validation: dot-separated labels of [a-z0-9-], no label edges
    // with '-', 1..=63 chars per label. Percent-encoding is NOT allowed in
    // authorities (internationalized names must be punycoded).
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(MurlParseError::InvalidAuthority(format!(
                "bad label in `{host}`"
            )));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(MurlParseError::InvalidAuthority(format!(
                "label `{label}` has characters outside [a-z0-9-] (IDN hosts must be punycoded)"
            )));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(MurlParseError::InvalidAuthority(format!(
                "label `{label}` starts or ends with '-'"
            )));
        }
    }
    Ok(Authority::Remote { host, port })
}

fn decode_segment(raw: &str) -> Result<String, MurlParseError> {
    if raw.is_empty() {
        return Err(MurlParseError::EmptySegment);
    }
    let bytes = percent_decode(raw)?;
    if bytes.len() > MAX_SEGMENT_BYTES {
        return Err(MurlParseError::SegmentTooLong);
    }
    let s = String::from_utf8(bytes)
        .map_err(|_| MurlParseError::InvalidSegment(format!("`{raw}` is not valid UTF-8")))?;
    for c in s.chars() {
        if c.is_control() {
            return Err(MurlParseError::InvalidSegment(format!(
                "`{raw}` decodes to a control character"
            )));
        }
        if c == '/' || c == '\\' {
            return Err(MurlParseError::InvalidSegment(format!(
                "`{raw}` decodes to a path separator"
            )));
        }
    }
    if s == "." || s == ".." {
        return Err(MurlParseError::InvalidSegment(
            "dot segments are forbidden".into(),
        ));
    }
    Ok(s)
}

fn parse_selector(f: &str) -> Result<Vec<SelectorItem>, MurlParseError> {
    if f.is_empty() {
        return Err(MurlParseError::InvalidSelector("empty fragment".into()));
    }
    let parts: Vec<&str> = f.split(',').collect();
    if parts.len() > MAX_SELECTOR_ITEMS {
        return Err(MurlParseError::InvalidSelector(format!(
            "at most {MAX_SELECTOR_ITEMS} comma-separated items"
        )));
    }
    let mut items = Vec::with_capacity(parts.len());
    for part in parts {
        let item = if let Some(role) = part.strip_prefix("role=") {
            if !crate::grammar::is_valid_role(role) {
                return Err(MurlParseError::InvalidSelector(format!(
                    "`{part}`: roles are [a-z0-9][a-z0-9-]{{0,31}}"
                )));
            }
            SelectorItem::Role(role.to_owned())
        } else if let Some(tag) = part.strip_prefix("tag=") {
            if !crate::grammar::is_valid_tag(tag) {
                return Err(MurlParseError::InvalidSelector(format!(
                    "`{part}`: tags are [a-z0-9-]{{1,32}}"
                )));
            }
            SelectorItem::Tag(tag.to_owned())
        } else {
            if !crate::grammar::is_valid_resource_id(part) {
                return Err(MurlParseError::InvalidSelector(format!(
                    "`{part}`: resource ids are [a-z0-9][a-z0-9_-]{{0,63}}"
                )));
            }
            SelectorItem::Id(part.to_owned())
        };
        items.push(item);
    }
    Ok(items)
}

/// Decode percent-encoding. `+` is NOT treated as a space (mURLs are not form
/// data). Invalid or truncated escapes are hard errors.
pub(crate) fn percent_decode(s: &str) -> Result<Vec<u8>, MurlParseError> {
    let raw = s.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == b'%' {
            if i + 3 > raw.len() {
                return Err(MurlParseError::InvalidPercentEncoding(format!(
                    "truncated escape in `{s}`"
                )));
            }
            let hi = hex_val(raw[i + 1]);
            let lo = hex_val(raw[i + 2]);
            match (hi, lo) {
                (Some(h), Some(l)) => out.push((h << 4) | l),
                _ => {
                    return Err(MurlParseError::InvalidPercentEncoding(format!(
                        "bad hex digits in `{s}`"
                    )))
                }
            }
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    Ok(out)
}

/// Percent-encode a decoded segment for canonical display: unreserved
/// characters pass through, everything else is `%XX` (uppercase hex) over
/// UTF-8 bytes.
pub(crate) fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        if unreserved {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(
                char::from_digit((b >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit((b & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_local() {
        let m = Murl::parse("murl://local/project-x").unwrap();
        assert_eq!(m.authority, Authority::Local);
        assert_eq!(m.name, vec!["project-x"]);
        assert!(m.version.is_latest());
        assert_eq!(m.to_string(), "murl://local/project-x");
    }

    #[test]
    fn parses_remote_with_path_version_selector() {
        let m = Murl::parse("murl://Example.COM/team/project-x@1.4.2#monitoring").unwrap();
        assert_eq!(
            m.authority,
            Authority::Remote {
                host: "example.com".into(),
                port: None
            }
        );
        assert_eq!(m.name, vec!["team", "project-x"]);
        assert_eq!(m.version, VersionTag::Pinned(vec![1, 4, 2]));
        assert_eq!(m.selector_display().as_deref(), Some("monitoring"));
        assert_eq!(m.identity(), "murl://example.com/team/project-x@1.4.2");
        assert_eq!(
            m.to_string(),
            "murl://example.com/team/project-x@1.4.2#monitoring"
        );
    }

    #[test]
    fn parses_loopback_with_port() {
        let m = Murl::parse("murl://127.0.0.1:8443/dev").unwrap();
        assert!(m.authority.is_loopback());
    }

    #[test]
    fn rejects_userinfo_phishing() {
        // The classic `trusted@attacker` trick must die at the parser.
        assert_eq!(
            Murl::parse("murl://github.com@evil.example/x").unwrap_err(),
            MurlParseError::UserinfoForbidden
        );
    }

    #[test]
    fn rejects_dot_segments_and_encoded_traversal() {
        assert!(Murl::parse("murl://local/../etc").is_err());
        assert!(Murl::parse("murl://local/%2e%2e/etc").is_err());
        assert!(Murl::parse("murl://local/a/%2e%2e").is_err());
        assert!(Murl::parse("murl://local/a%2Fb").is_err()); // encoded slash
        assert!(Murl::parse("murl://local/a%5Cb").is_err()); // encoded backslash
    }

    #[test]
    fn rejects_control_and_nul() {
        assert!(Murl::parse("murl://local/a%00b").is_err());
        assert!(Murl::parse("murl://local/a%0Ab").is_err());
        assert!(Murl::parse("murl://local/a b").is_err());
        assert!(Murl::parse("murl://local/caf\u{00e9}").is_err()); // raw non-ASCII
    }

    #[test]
    fn accepts_percent_encoded_utf8() {
        let m = Murl::parse("murl://local/caf%C3%A9").unwrap();
        assert_eq!(m.name, vec!["café"]);
        assert_eq!(m.identity(), "murl://local/caf%C3%A9");
        // Round-trip.
        assert_eq!(Murl::parse(&m.identity()).unwrap(), m);
    }

    #[test]
    fn rejects_bad_versions() {
        assert!(Murl::parse("murl://local/x@").is_err());
        assert!(Murl::parse("murl://local/x@1.2.3.4").is_err());
        assert!(Murl::parse("murl://local/x@01").is_err());
        assert!(Murl::parse("murl://local/x@1..2").is_err());
        assert!(Murl::parse("murl://local/x@123456").is_err());
        assert!(Murl::parse("murl://local/x@a.b").is_err());
        assert!(Murl::parse("murl://local/x@1@2").is_err());
        assert!(Murl::parse("murl://local/a@1/b").is_err()); // version not on last segment
    }

    #[test]
    fn version_latest_is_default_and_elided() {
        let a = Murl::parse("murl://local/x").unwrap();
        let b = Murl::parse("murl://local/x@latest").unwrap();
        assert_eq!(a.identity(), b.identity());
    }

    #[test]
    fn rejects_structural_garbage() {
        assert!(Murl::parse("").is_err());
        assert!(Murl::parse("murl://").is_err());
        assert!(Murl::parse("murl://local").is_err());
        assert!(Murl::parse("murl://local/").is_err());
        assert!(Murl::parse("murl://local//x").is_err());
        assert!(Murl::parse("murl://local/x/").is_err());
        assert!(Murl::parse("https://example.com/x").is_err());
        assert!(Murl::parse("murl:local/x").is_err());
        assert!(Murl::parse("murl://[::1]/x").is_err());
        assert!(Murl::parse("murl://local:80/x").is_err());
        assert!(Murl::parse(&format!("murl://local/{}", "a".repeat(2000))).is_err());
        let deep = format!("murl://local/{}", vec!["a"; 20].join("/"));
        assert!(Murl::parse(&deep).is_err());
    }

    #[test]
    fn rejects_bad_selectors() {
        assert!(Murl::parse("murl://local/x#").is_err());
        assert!(Murl::parse("murl://local/x#UPPER").is_err());
        assert!(Murl::parse("murl://local/x#a b").is_err());
        assert!(Murl::parse("murl://local/x#a/b").is_err());
        assert!(Murl::parse("murl://local/x#a,,b").is_err());
        assert!(Murl::parse("murl://local/x#role=").is_err());
        assert!(Murl::parse("murl://local/x#role=UPPER").is_err());
        assert!(Murl::parse("murl://local/x#tag=x_y").is_err());
        assert!(Murl::parse("murl://local/x#a,b,c,d,e,f,g,h,i").is_err()); // 9 items
    }

    #[test]
    fn parses_selector_items_and_round_trips() {
        let m = Murl::parse("murl://local/x#docs,role=monitoring,tag=dev").unwrap();
        assert_eq!(
            m.selector.as_deref(),
            Some(
                &[
                    SelectorItem::Id("docs".into()),
                    SelectorItem::Role("monitoring".into()),
                    SelectorItem::Tag("dev".into()),
                ][..]
            )
        );
        assert_eq!(m.to_string(), "murl://local/x#docs,role=monitoring,tag=dev");
        assert_eq!(Murl::parse(&m.to_string()).unwrap(), m);
        // Identity still strips the selector.
        assert_eq!(m.identity(), "murl://local/x");
    }

    #[test]
    fn identity_strips_selector_and_query() {
        let m = Murl::parse("murl://local/x?profile=min#docs").unwrap();
        assert_eq!(m.identity(), "murl://local/x");
        assert_eq!(m.to_string(), "murl://local/x?profile=min#docs");
    }

    #[test]
    fn rejects_bad_percent_encoding() {
        assert!(Murl::parse("murl://local/a%").is_err());
        assert!(Murl::parse("murl://local/a%2").is_err());
        assert!(Murl::parse("murl://local/a%zz").is_err());
        assert!(Murl::parse("murl://local/a%C3%28").is_err()); // invalid UTF-8
    }

    #[test]
    fn authority_validation() {
        assert!(Murl::parse("murl://-bad.example/x").is_err());
        assert!(Murl::parse("murl://bad-.example/x").is_err());
        assert!(Murl::parse("murl://ex..ample/x").is_err());
        assert!(Murl::parse("murl://.example/x").is_err());
        assert!(Murl::parse("murl://example./x").is_err());
        assert!(Murl::parse("murl://exa_mple/x").is_err());
        assert!(Murl::parse("murl://example.com:0/x").is_err());
        assert!(Murl::parse("murl://example.com:99999/x").is_err());
        let long = format!("murl://{}/x", "a".repeat(300));
        assert!(Murl::parse(&long).is_err());
    }
}
