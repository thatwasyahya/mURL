//! WebAssembly bindings for `murl-core`.
//!
//! The playground on the documentation site builds and validates manifests
//! by calling **this**, not by re-implementing the rules in JavaScript. That
//! matters more than it might look: the project already keeps one
//! independent implementation honest with a conformance suite, and a second
//! copy of the grammar written in a hurry for a web page — drifting quietly
//! from `spec/SPECIFICATION.md` every time the spec moved — is exactly the
//! kind of seam this codebase has been bitten by before.
//!
//! `murl-core` was built for this: no network, no process launching, no
//! filesystem in the paths used here. Compiling it to wasm is the payoff for
//! that discipline rather than a new capability.
//!
//! # ABI
//!
//! Three exports, because a wasm function can only pass integers:
//!
//! * `murl_alloc(len) -> ptr` — reserve `len` bytes for the caller to fill
//! * `murl_free(ptr, len)` — release a buffer
//! * `murl_process(ptr, len) -> ptr` — read UTF-8 JSON in, return a buffer
//!   whose first four bytes are a little-endian length followed by UTF-8
//!   JSON out. The caller frees it with `murl_free(ptr, 4 + length)`.

use murl_core::kind::Kind;
use murl_core::limits::Limits;
use murl_core::manifest::{validate_target, Manifest};
use murl_core::murl::Murl;
use murl_core::policy::classify;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ---------------------------------------------------------------- ABI ----

/// Reserve `len` bytes and hand the pointer to the caller.
#[no_mangle]
pub extern "C" fn murl_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Release a buffer previously produced by [`murl_alloc`] or
/// [`murl_process`].
///
/// # Safety
/// `ptr` must have come from this module and `len` must be the length it was
/// created with; the buffer must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn murl_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // SAFETY: the contract above; Vec::from_raw_parts reclaims exactly what
    // murl_alloc leaked, with capacity equal to length in both cases.
    drop(unsafe { Vec::from_raw_parts(ptr, len, len) });
}

/// Read a JSON request and return a length-prefixed JSON response.
///
/// # Safety
/// `ptr`/`len` must describe an initialised buffer of UTF-8 bytes obtained
/// from [`murl_alloc`].
#[no_mangle]
pub unsafe extern "C" fn murl_process(ptr: *const u8, len: usize) -> *mut u8 {
    // SAFETY: the contract above.
    let input = unsafe { std::slice::from_raw_parts(ptr, len) };
    let response = match std::str::from_utf8(input) {
        Ok(text) => process(text),
        Err(_) => error_response("request was not valid UTF-8"),
    };

    let bytes = response.into_bytes();
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes);
    let out_ptr = out.as_mut_ptr();
    std::mem::forget(out);
    out_ptr
}

// ------------------------------------------------------------- request ----

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Request {
    /// Human name of the destination.
    name: String,
    description: String,
    /// The local name to install under, e.g. `project-x`.
    slug: String,
    /// The authority to publish under, for the hosting instructions.
    authority: String,
    /// One pasted line per resource.
    lines: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceView {
    line: String,
    id: String,
    kind: String,
    target: String,
    tier: String,
    /// Why this line could not become a resource, if it could not.
    error: Option<String>,
}

fn error_response(message: &str) -> String {
    json!({ "ok": false, "fatal": message }).to_string()
}

// ------------------------------------------------------------ the work ----

fn process(text: &str) -> String {
    let request: Request = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => return error_response(&format!("malformed request: {e}")),
    };

    let mut views: Vec<ResourceView> = Vec::new();
    let mut resources: Vec<Value> = Vec::new();
    let mut used_ids: Vec<String> = Vec::new();

    for raw in &request.lines {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match interpret(line) {
            Err(problem) => views.push(ResourceView {
                line: line.to_owned(),
                id: String::new(),
                kind: String::new(),
                target: line.to_owned(),
                tier: String::new(),
                error: Some(problem),
            }),
            Ok((kind, target)) => {
                let id = unique_id(&target, &kind, &mut used_ids);
                let tier = classify(&kind, &target).to_string();
                let mut entry = json!({
                    "id": id,
                    "kind": kind.to_string(),
                    "target": target,
                });
                // A role is a guess, so it is only offered when the kind
                // makes it unambiguous.
                if let Some(role) = suggested_role(&kind, &target) {
                    entry["role"] = json!(role);
                }
                resources.push(entry);
                views.push(ResourceView {
                    line: line.to_owned(),
                    id,
                    kind: kind.to_string(),
                    target,
                    tier,
                    error: None,
                });
            }
        }
    }

    let slug = sanitize_slug(&request.slug, &request.name);
    let authority = request.authority.trim();
    let identity = format!("murl://local/{slug}");

    if resources.is_empty() {
        return json!({
            "ok": false,
            "resources": views,
            "errors": ["a manifest needs at least one resource"],
            "warnings": [],
            "murl": identity,
        })
        .to_string();
    }

    let mut manifest = json!({
        "murlVersion": "0.2",
        "id": identity,
        "name": if request.name.trim().is_empty() { slug.clone() } else { request.name.trim().to_owned() },
        "resources": resources,
    });
    if !request.description.trim().is_empty() {
        manifest["description"] = json!(request.description.trim());
    }

    // Round-trip through the real parser and validator. Everything the
    // playground claims about a manifest is claimed by murl-core.
    let bytes = serde_json::to_vec(&manifest).unwrap_or_default();
    let (errors, warnings) = match Manifest::from_slice(&bytes, &Limits::default()) {
        Err(e) => (vec![e.to_string()], Vec::new()),
        Ok(parsed) => {
            let report = parsed.validate();
            (
                report
                    .errors
                    .iter()
                    .map(|i| format!("{}: {}", i.path, i.message))
                    .collect(),
                report
                    .warnings
                    .iter()
                    .map(|i| format!("{}: {}", i.path, i.message))
                    .collect(),
            )
        }
    };

    let pretty = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    let mut published = Value::Null;
    if !authority.is_empty() {
        // Only claim a remote identity if it would actually parse.
        let candidate = format!("murl://{authority}/{slug}");
        if Murl::parse(&candidate).is_ok() {
            published = json!({
                "murl": candidate,
                "path": format!(".well-known/murl/{slug}.murl.json"),
                "url": format!("https://{authority}/.well-known/murl/{slug}.murl.json"),
            });
        }
    }

    json!({
        "ok": errors.is_empty(),
        "resources": views,
        "errors": errors,
        "warnings": warnings,
        "manifest": pretty,
        "murl": identity,
        "slug": slug,
        "filename": format!("{slug}.murl.json"),
        "published": published,
    })
    .to_string()
}

/// Turn one pasted line into a (kind, target) pair.
///
/// A leading kind word wins (`terminal ~/projects/x`); otherwise the scheme
/// or the shape of the path decides.
fn interpret(line: &str) -> Result<(Kind, String), String> {
    if let Some((head, rest)) = line.split_once(char::is_whitespace) {
        if let Ok(kind) = Kind::parse(head.trim()) {
            let target = rest.trim().to_owned();
            return check(kind, target);
        }
    }

    let lower = line.to_ascii_lowercase();
    let kind = if lower.starts_with("https://") || lower.starts_with("http://") {
        Kind::Https
    } else if lower.starts_with("murl://") {
        Kind::Murl
    } else if lower.starts_with("ssh://") {
        Kind::Ssh
    } else if lower.starts_with("rdp://") || lower.starts_with("vnc://") {
        Kind::RemoteDesktop
    } else if lower.starts_with("mailto:") {
        Kind::Mailto
    } else if lower.starts_with("geo:") {
        Kind::Geo
    } else if line.starts_with('/') || line.starts_with('~') || is_windows_path(line) {
        // A directory unless it looks like a file: this is a guess, and the
        // playground says so rather than pretending otherwise.
        if looks_like_file(line) {
            Kind::File
        } else {
            Kind::Dir
        }
    } else if looks_like_hostname(&lower) {
        // A bare host is the commonest paste; https is the safe reading.
        return check(
            Kind::Https,
            format!("https://{}", line.trim_start_matches("www.")),
        );
    } else {
        return Err(
            "not recognised — use a URL, an absolute or ~ path, or prefix the line with a kind \
             such as `terminal ~/projects/x`"
                .into(),
        );
    };

    check(kind, line.to_owned())
}

/// Whether a bare token reads as a hostname, optionally with a path.
///
/// Without this, anything containing a dot became `https://<whatever>` —
/// `../etc/passwd` included, which then passed target validation because a
/// host of `..` is syntactically a reg-name. A guess that turns a path
/// traversal into a URL is a bad guess.
fn looks_like_hostname(line: &str) -> bool {
    let host = line.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    if host.is_empty() || !host.contains('.') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

fn check(kind: Kind, target: String) -> Result<(Kind, String), String> {
    match validate_target(&kind, &target) {
        Ok(()) => Ok((kind, target)),
        Err(why) => Err(why),
    }
}

fn is_windows_path(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

fn looks_like_file(line: &str) -> bool {
    if line.ends_with('/') || line.ends_with('\\') {
        return false;
    }
    let last = line.rsplit(['/', '\\']).next().unwrap_or("");
    // A dot that is not the first character reads as an extension.
    matches!(last.rfind('.'), Some(i) if i > 0 && i < last.len() - 1)
}

fn suggested_role(kind: &Kind, target: &str) -> Option<&'static str> {
    let t = target.to_ascii_lowercase();
    match kind {
        Kind::Https => {
            if t.contains("github.com") || t.contains("gitlab") || t.contains("bitbucket") {
                Some("source")
            } else if t.contains("docs") || t.contains("wiki") || t.contains("readthedocs") {
                Some("docs")
            } else if t.contains("jira") || t.contains("issues") || t.contains("linear") {
                Some("issues")
            } else if t.contains("grafana") || t.contains("datadog") || t.contains("sentry") {
                Some("monitoring")
            } else {
                None
            }
        }
        Kind::Dir => Some("workspace"),
        Kind::Terminal => Some("terminal"),
        _ => None,
    }
}

/// A resource id derived from the target: `[a-z0-9][a-z0-9_-]{0,63}`,
/// unique within the manifest.
fn unique_id(target: &str, kind: &Kind, used: &mut Vec<String>) -> String {
    let mut base = slug_from_target(target, kind);
    if base.is_empty() {
        base = "resource".to_owned();
    }
    let mut candidate = base.clone();
    let mut n = 2;
    while used.iter().any(|u| u == &candidate) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    used.push(candidate.clone());
    candidate
}

fn slug_from_target(target: &str, kind: &Kind) -> String {
    // A terminal is named for what it is, not for the directory it opens in:
    // otherwise it collides with the workspace resource pointing at the same
    // path and ends up as a bare numeric suffix.
    if matches!(kind, Kind::Terminal) {
        return "terminal".to_owned();
    }
    let stripped = target
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(target);
    // For a URL prefer the host's first label; for a path, the last segment.
    let candidate = if matches!(
        kind,
        Kind::Https | Kind::Ssh | Kind::RemoteDesktop | Kind::Murl
    ) {
        let host = stripped.split(['/', '?', '#', ':']).next().unwrap_or("");
        host.split('.').next().unwrap_or(host).to_owned()
    } else {
        stripped
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("")
            .to_owned()
    };
    sanitize_id(&candidate)
}

fn sanitize_id(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.to_ascii_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if (c == '-' || c == '_' || c == ' ') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 64 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_owned();
    // The first character must be alphanumeric.
    match trimmed.chars().next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => trimmed,
        _ => String::new(),
    }
}

fn sanitize_slug(slug: &str, name: &str) -> String {
    let from_slug = sanitize_id(slug);
    if !from_slug.is_empty() {
        return from_slug;
    }
    let from_name = sanitize_id(name);
    if !from_name.is_empty() {
        return from_name;
    }
    "destination".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &[&str]) -> Value {
        let request = json!({
            "name": "Test",
            "slug": "test",
            "lines": lines,
        });
        serde_json::from_str(&process(&request.to_string())).unwrap()
    }

    #[test]
    fn infers_kinds_from_what_was_pasted() {
        let out = run(&[
            "https://github.com/acme/project-x",
            "~/projects/project-x",
            "~/notes/plan.md",
            "terminal ~/projects/project-x",
            "ssh://build@ci.example",
            "mailto:team@example.com",
        ]);
        let kinds: Vec<&str> = out["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            vec!["https", "dir", "file", "terminal", "ssh", "mailto"]
        );
        assert_eq!(out["ok"], true);
    }

    #[test]
    fn classifies_with_the_real_policy() {
        let out = run(&[
            "https://e.example",
            "~/notes.txt",
            "terminal ~/x",
            "~/setup.sh",
        ]);
        let tiers: Vec<&str> = out["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["tier"].as_str().unwrap())
            .collect();
        // The executable extension escalates, exactly as in the CLI.
        assert_eq!(tiers, vec!["SAFE", "SENSITIVE", "DANGEROUS", "DANGEROUS"]);
    }

    #[test]
    fn rejects_targets_the_validator_rejects() {
        let out = run(&["http://example.com/x", "relative/path", "../etc/passwd"]);
        let errors: Vec<bool> = out["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| !r["error"].is_null())
            .collect();
        assert_eq!(errors, vec![true, true, true]);
    }

    #[test]
    fn ids_are_unique_and_well_formed() {
        let out = run(&[
            "https://github.com/a",
            "https://github.com/b",
            "https://github.com/c",
        ]);
        let ids: Vec<&str> = out["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["github", "github-2", "github-3"]);
    }

    #[test]
    fn output_manifest_is_valid_by_the_real_validator() {
        let out = run(&["https://e.example/x", "~/projects/x"]);
        assert_eq!(out["errors"].as_array().unwrap().len(), 0);
        let text = out["manifest"].as_str().unwrap();
        let parsed = Manifest::from_slice(text.as_bytes(), &Limits::default()).unwrap();
        assert!(parsed.validate().is_valid());
    }

    #[test]
    fn a_bare_host_becomes_https() {
        let out = run(&["example.com/docs"]);
        assert_eq!(out["resources"][0]["kind"], "https");
        assert_eq!(out["resources"][0]["target"], "https://example.com/docs");
    }

    #[test]
    fn empty_input_is_an_error_not_an_empty_manifest() {
        let out = run(&[]);
        assert_eq!(out["ok"], false);
    }

    #[test]
    fn publishing_identity_only_when_the_authority_parses() {
        let req =
            json!({"name":"T","slug":"t","authority":"acme.example","lines":["https://e.example"]});
        let out: Value = serde_json::from_str(&process(&req.to_string())).unwrap();
        assert_eq!(out["published"]["murl"], "murl://acme.example/t");

        let req =
            json!({"name":"T","slug":"t","authority":"not a host","lines":["https://e.example"]});
        let out: Value = serde_json::from_str(&process(&req.to_string())).unwrap();
        assert!(out["published"].is_null());
    }
}
