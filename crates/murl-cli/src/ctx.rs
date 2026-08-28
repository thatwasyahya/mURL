//! Application context: directories, configuration, and resolver wiring.

use std::cell::RefCell;
use std::path::PathBuf;

use murl_core::cache::ManifestCache;
use murl_core::dispatch::OpenerConfig;
use murl_core::error::{Error, Result};
use murl_core::fetch::{LocalStore, RemoteFetcher};
use murl_core::limits::Limits;
use murl_core::manifest::Manifest;
use murl_core::murl::Murl;
use murl_core::policy::Policy;
use murl_core::resolver::{Origin, Resolution, Resolver};
use murl_core::time::SystemClock;
use murl_core::trust::TrustStore;

use crate::logger;
use crate::paths::{home_dir, AppPaths};
use murl_net::HttpsFetcher;

// Configuration types live in murl-core so the CLI and the daemon read
// them through one loader (see murl_core::config).
pub use murl_core::config::HandlersFile;

pub fn load_handlers(path: &std::path::Path) -> Result<HandlersFile> {
    HandlersFile::load(path)
}

pub fn save_handlers(path: &std::path::Path, handlers: &HandlersFile) -> Result<()> {
    handlers.save(path)
}

/// A target argument: an mURL or a manifest file path.
#[derive(Debug, Clone)]
pub enum TargetRef {
    Murl(Box<Murl>),
    File(PathBuf),
}

pub fn parse_target(s: &str) -> Result<TargetRef> {
    if s.len() >= 5 && s[..5].eq_ignore_ascii_case("murl:") {
        Ok(TargetRef::Murl(Box::new(Murl::parse(s)?)))
    } else {
        Ok(TargetRef::File(PathBuf::from(s)))
    }
}

#[derive(Debug)]
pub struct App {
    pub paths: AppPaths,
    pub limits: Limits,
    pub policy: Policy,
    pub opener: OpenerConfig,
    pub store: LocalStore,
    pub cache: ManifestCache,
    pub trust: RefCell<TrustStore>,
    pub clock: SystemClock,
    pub json: bool,
    pub offline: bool,
    pub refresh: bool,
}

impl App {
    pub fn init(json: bool, offline: bool, refresh: bool) -> Result<App> {
        let paths = AppPaths::discover()?;

        let config = murl_core::config::UserConfig::load(&paths.config_file())?;
        let limits = config.limits();
        let policy = config.policy();

        let handlers = HandlersFile::load(&paths.handlers_file())?;
        let opener = handlers.to_opener(std::env::consts::OS, home_dir());

        let store = LocalStore::new(paths.names_dir());
        let cache = ManifestCache::new(paths.manifest_cache_dir());
        let trust = RefCell::new(TrustStore::load(paths.trust_file())?);

        Ok(App {
            paths,
            limits,
            policy,
            opener,
            store,
            cache,
            trust,
            clock: SystemClock,
            json,
            offline,
            refresh,
        })
    }

    /// Run `f` with a fully wired resolver. The fetcher and the trust borrow
    /// live exactly as long as the closure.
    pub fn with_resolver<T>(&self, f: impl FnOnce(&Resolver<'_>) -> Result<T>) -> Result<T> {
        let fetcher = HttpsFetcher::with_trace(logger::debug);
        let trust = self.trust.borrow();
        let remote: Option<&dyn RemoteFetcher> = if self.offline { None } else { Some(&fetcher) };
        let resolver = Resolver {
            local_store: &self.store,
            remote,
            cache: Some(&self.cache),
            trust_store: &trust,
            limits: self.limits.clone(),
            clock: &self.clock,
        };
        f(&resolver)
    }

    /// Full resolution of a target (mURL or file), honoring `--refresh`.
    pub fn resolve_target(&self, target: &str) -> Result<Resolution> {
        match parse_target(target)? {
            TargetRef::Murl(m) => {
                if self.refresh && self.cache.evict(&m.identity()).unwrap_or(false) {
                    logger::debug(&format!("evicted cache entry for {}", m.identity()));
                }
                self.with_resolver(|r| r.resolve(&m))
            }
            TargetRef::File(path) => self.with_resolver(|r| r.resolve_file(&path)),
        }
    }

    /// Fetch just the root manifest of a target, without validation.
    /// Returns the manifest, its origin, resolver warnings, and the parsed
    /// mURL when the target was one.
    pub fn fetch_root_of(
        &self,
        target: &str,
    ) -> Result<(Manifest, Origin, Vec<String>, Option<Murl>)> {
        match parse_target(target)? {
            TargetRef::Murl(m) => {
                if self.refresh {
                    self.cache.evict(&m.identity()).ok();
                }
                let (manifest, origin, warnings) = self.with_resolver(|r| r.fetch_root(&m))?;
                Ok((manifest, origin, warnings, Some(*m)))
            }
            TargetRef::File(path) => {
                let bytes = std::fs::read(&path)
                    .map_err(|e| Error::NotFound(format!("cannot read {}: {e}", path.display())))?;
                let manifest = Manifest::from_slice(&bytes, &self.limits)?;
                Ok((manifest, Origin::LocalFile(path), Vec::new(), None))
            }
        }
    }
}
