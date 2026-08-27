//! Manifest cache for remote resolutions.
//!
//! Entries are keyed by the sha256 of the mURL's canonical identity and
//! stored as two files: the manifest bytes verbatim, plus a metadata sidecar
//! (source URL, fetch time, content hash). The content hash is verified on
//! every read; an entry that fails verification is treated as a miss and
//! removed. Cache policy (`docs/resolution.md` §Offline):
//!
//! * `@latest` entries are fresh for `limits.cache_ttl_secs`.
//! * pinned-version entries are immutable and never expire.
//! * a stale entry is still usable as an explicit offline fallback — the
//!   resolver surfaces a warning when it does so.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::trust::sha256_hex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheMeta {
    pub identity: String,
    pub url: String,
    pub fetched_at: u64,
    pub sha256: String,
}

/// A cache read: bytes plus provenance and freshness.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub bytes: Vec<u8>,
    pub meta: CacheMeta,
    pub fresh: bool,
}

#[derive(Debug, Clone)]
pub struct ManifestCache {
    root: PathBuf,
}

impl ManifestCache {
    pub fn new(root: PathBuf) -> ManifestCache {
        ManifestCache { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn paths(&self, identity: &str) -> (PathBuf, PathBuf) {
        let key = sha256_hex(identity.as_bytes());
        (
            self.root.join(format!("{key}.murl.json")),
            self.root.join(format!("{key}.meta.json")),
        )
    }

    /// Read an entry. `ttl_secs` of `u64::MAX` means "never stale" (pinned
    /// versions). Integrity failures behave as a miss.
    pub fn get(&self, identity: &str, ttl_secs: u64, now: u64) -> Option<CacheEntry> {
        let (data_path, meta_path) = self.paths(identity);
        let bytes = std::fs::read(&data_path).ok()?;
        let meta: CacheMeta = serde_json::from_slice(&std::fs::read(&meta_path).ok()?).ok()?;
        if meta.identity != identity || sha256_hex(&bytes) != meta.sha256 {
            // Corrupted or crossed entry: drop it.
            std::fs::remove_file(&data_path).ok();
            std::fs::remove_file(&meta_path).ok();
            return None;
        }
        let fresh = match ttl_secs {
            u64::MAX => true,
            ttl => now < meta.fetched_at.saturating_add(ttl),
        };
        Some(CacheEntry { bytes, meta, fresh })
    }

    pub fn put(&self, identity: &str, url: &str, bytes: &[u8], now: u64) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        let (data_path, meta_path) = self.paths(identity);
        let meta = CacheMeta {
            identity: identity.to_owned(),
            url: url.to_owned(),
            fetched_at: now,
            sha256: sha256_hex(bytes),
        };
        std::fs::write(&data_path, bytes)?;
        std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
        Ok(())
    }

    /// Remove one identity from the cache. Returns whether it existed.
    pub fn evict(&self, identity: &str) -> Result<bool> {
        let (data_path, meta_path) = self.paths(identity);
        let existed = data_path.exists() || meta_path.exists();
        if data_path.exists() {
            std::fs::remove_file(&data_path)?;
        }
        if meta_path.exists() {
            std::fs::remove_file(&meta_path)?;
        }
        Ok(existed)
    }

    pub fn list(&self) -> Result<Vec<CacheMeta>> {
        let mut out = Vec::new();
        if !self.root.exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".meta.json") {
                if let Ok(bytes) = std::fs::read(entry.path()) {
                    if let Ok(meta) = serde_json::from_slice::<CacheMeta>(&bytes) {
                        out.push(meta);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.identity.cmp(&b.identity));
        Ok(out)
    }

    /// Clear everything. Returns the number of entries removed.
    pub fn clear(&self) -> Result<usize> {
        let entries = self.list()?;
        for meta in &entries {
            self.evict(&meta.identity)?;
        }
        Ok(entries.len())
    }
}

impl ManifestCache {
    /// Map an error into a warning string, for callers that treat cache
    /// failures as soft.
    pub fn describe_err(e: &Error) -> String {
        format!("cache error (ignored): {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache(tag: &str) -> ManifestCache {
        let dir = std::env::temp_dir().join(format!("murl-cache-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        ManifestCache::new(dir)
    }

    #[test]
    fn roundtrip_and_freshness() {
        let cache = temp_cache("rt");
        let id = "murl://example.com/p";
        assert!(cache.get(id, 60, 1000).is_none());
        cache
            .put(
                id,
                "https://example.com/.well-known/murl/p.murl.json",
                b"{}",
                1000,
            )
            .unwrap();
        let e = cache.get(id, 60, 1030).unwrap();
        assert!(e.fresh);
        let e = cache.get(id, 60, 2000).unwrap();
        assert!(!e.fresh);
        let e = cache.get(id, u64::MAX, u64::MAX - 1).unwrap();
        assert!(e.fresh, "pinned entries never go stale");
        std::fs::remove_dir_all(cache.root()).ok();
    }

    #[test]
    fn tampered_entries_are_dropped() {
        let cache = temp_cache("tamper");
        let id = "murl://example.com/p";
        cache.put(id, "https://x", b"{\"a\":1}", 0).unwrap();
        let (data_path, _) = cache.paths(id);
        std::fs::write(&data_path, b"{\"a\":2}").unwrap();
        assert!(cache.get(id, u64::MAX, 0).is_none());
        // And the corrupt entry was evicted.
        assert!(!data_path.exists());
        std::fs::remove_dir_all(cache.root()).ok();
    }

    #[test]
    fn list_and_clear() {
        let cache = temp_cache("list");
        cache.put("murl://a.com/x", "https://a", b"{}", 0).unwrap();
        cache.put("murl://b.com/y", "https://b", b"{}", 0).unwrap();
        assert_eq!(cache.list().unwrap().len(), 2);
        assert_eq!(cache.clear().unwrap(), 2);
        assert!(cache.list().unwrap().is_empty());
        std::fs::remove_dir_all(cache.root()).ok();
    }
}
