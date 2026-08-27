//! Manifest sources: the local name store, and the trait remote fetching
//! plugs into.
//!
//! murl-core performs no network I/O. Remote resolution is abstracted behind
//! [`RemoteFetcher`] so that the embedder (CLI, daemon, tests) owns the HTTP
//! stack, its TLS policy, its timeout enforcement, and its SSRF protections —
//! and so the entire resolver is testable hermetically.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::murl::{Authority, Murl, VersionTag};

/// Fetches manifest bytes for a URL. Implementations MUST:
///
/// * enforce `limits.max_manifest_bytes` while reading (not after),
/// * enforce `limits.fetch_timeout_secs`,
/// * refuse redirects beyond `limits.max_redirects` (0 by the spec),
/// * refuse non-loopback plain HTTP.
pub trait RemoteFetcher: std::fmt::Debug {
    fn fetch(&self, url: &str, limits: &Limits) -> Result<Vec<u8>>;
}

/// Compute the well-known manifest URL for a remote mURL (spec §6.2):
///
/// ```text
/// murl://example.com/team/project-x        -> https://example.com/.well-known/murl/team/project-x.murl.json
/// murl://example.com/team/project-x@1.4.2  -> https://example.com/.well-known/murl/team/project-x@1.4.2.murl.json
/// murl://localhost:8080/dev                -> http://localhost:8080/.well-known/murl/dev.murl.json
/// ```
///
/// Returns `None` for the `local` authority.
pub fn well_known_url(murl: &Murl) -> Option<String> {
    let Authority::Remote { host, port } = &murl.authority else {
        return None;
    };
    let scheme = if murl.authority.is_loopback() {
        "http"
    } else {
        "https"
    };
    let hostport = match port {
        Some(p) => format!("{host}:{p}"),
        None => host.clone(),
    };
    let mut path = String::new();
    for seg in &murl.name {
        path.push('/');
        path.push_str(&crate::murl::encode_segment(seg));
    }
    let suffix = match &murl.version {
        VersionTag::Latest => String::new(),
        pinned => format!("@{pinned}"),
    };
    Some(format!(
        "{scheme}://{hostport}/.well-known/murl{path}{suffix}.murl.json"
    ))
}

/// The local name store: `murl://local/...` names mapped to manifest files
/// under a user-owned directory (`<data>/names/`).
///
/// Layout mirrors name segments as directories; the final segment becomes
/// `<segment>[@version].murl.json`. Segment bytes outside `[A-Za-z0-9._-]`
/// are `%XX`-escaped so any valid mURL name has a filesystem-safe path.
#[derive(Debug, Clone)]
pub struct LocalStore {
    root: PathBuf,
}

impl LocalStore {
    pub fn new(root: PathBuf) -> LocalStore {
        LocalStore { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The manifest file path for a local name.
    pub fn manifest_path(&self, murl: &Murl) -> PathBuf {
        let mut path = self.root.clone();
        for seg in &murl.name[..murl.name.len() - 1] {
            path.push(sanitize_component(seg));
        }
        let last = sanitize_component(murl.short_name());
        let suffix = match &murl.version {
            VersionTag::Latest => String::new(),
            pinned => format!("@{pinned}"),
        };
        path.push(format!("{last}{suffix}.murl.json"));
        path
    }

    /// Load manifest bytes for a local name.
    pub fn load(&self, murl: &Murl) -> Result<(Vec<u8>, PathBuf)> {
        let path = self.manifest_path(murl);
        match std::fs::read(&path) {
            Ok(bytes) => Ok((bytes, path)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::NotFound(format!(
                "`{}` is not in the local name store (expected {}); add it with `murl name add`",
                murl.identity(),
                path.display()
            ))),
            Err(e) => Err(e.into()),
        }
    }

    /// Install manifest bytes under a local name. Bytes are stored verbatim
    /// (a signed manifest must survive the store byte-for-byte).
    pub fn add(&self, murl: &Murl, bytes: &[u8]) -> Result<PathBuf> {
        let path = self.manifest_path(murl);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    /// Remove a local name. Returns whether it existed.
    pub fn remove(&self, murl: &Murl) -> Result<bool> {
        let path = self.manifest_path(murl);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// List every stored name as a canonical `murl://local/...` identity.
    pub fn list(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        if self.root.exists() {
            walk(&self.root, &mut Vec::new(), &mut out)?;
        }
        out.sort();
        Ok(out)
    }
}

fn walk(dir: &Path, components: &mut Vec<String>, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let ftype = entry.file_type()?;
        if ftype.is_dir() {
            components.push(name);
            walk(&entry.path(), components, out)?;
            components.pop();
        } else if let Some(stem) = name.strip_suffix(".murl.json") {
            let mut identity = String::from("murl://local");
            for c in components.iter() {
                identity.push('/');
                identity.push_str(c);
            }
            identity.push('/');
            identity.push_str(stem);
            // Only report entries that decode back to a valid mURL; foreign
            // files in the store directory are ignored, not trusted.
            if Murl::parse(&identity).is_ok() {
                out.push(identity);
            }
        }
    }
    Ok(())
}

/// Escape a decoded name segment into a filesystem-safe component.
fn sanitize_component(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    for b in seg.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn murl(s: &str) -> Murl {
        Murl::parse(s).unwrap()
    }

    #[test]
    fn well_known_urls() {
        assert_eq!(
            well_known_url(&murl("murl://example.com/team/project-x")).unwrap(),
            "https://example.com/.well-known/murl/team/project-x.murl.json"
        );
        assert_eq!(
            well_known_url(&murl("murl://example.com/p@1.4.2")).unwrap(),
            "https://example.com/.well-known/murl/p@1.4.2.murl.json"
        );
        assert_eq!(
            well_known_url(&murl("murl://localhost:8080/dev")).unwrap(),
            "http://localhost:8080/.well-known/murl/dev.murl.json"
        );
        assert_eq!(well_known_url(&murl("murl://local/x")), None);
    }

    #[test]
    fn store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("murl-store-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = LocalStore::new(dir.clone());
        let m = murl("murl://local/team/project-x");
        assert!(store.load(&m).is_err());
        store.add(&m, b"{}").unwrap();
        let (bytes, _) = store.load(&m).unwrap();
        assert_eq!(bytes, b"{}");
        assert_eq!(store.list().unwrap(), vec!["murl://local/team/project-x"]);
        assert!(store.remove(&m).unwrap());
        assert!(!store.remove(&m).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn versioned_names_are_distinct_files() {
        let store = LocalStore::new(PathBuf::from("/data/names"));
        let a = store.manifest_path(&murl("murl://local/p"));
        let b = store.manifest_path(&murl("murl://local/p@1.2.0"));
        assert_ne!(a, b);
        assert!(a.to_string_lossy().ends_with("p.murl.json"));
        assert!(b.to_string_lossy().ends_with("p@1.2.0.murl.json"));
    }

    #[test]
    fn hostile_segments_are_escaped() {
        let store = LocalStore::new(PathBuf::from("/data/names"));
        // `%2E%2E` decodes to `..` — the parser already rejects it, so build
        // an adjacent hostile-ish name that parses: `...`
        let m = murl("murl://local/...");
        let p = store.manifest_path(&m);
        assert!(p.to_string_lossy().contains("...murl.json"));
        // Unicode and specials are escaped.
        let m = murl("murl://local/caf%C3%A9%20x");
        let p = store.manifest_path(&m).to_string_lossy().into_owned();
        assert!(!p.contains(' '), "{p}");
        assert!(p.contains("caf%C3%A9%20x"), "{p}");
    }
}
