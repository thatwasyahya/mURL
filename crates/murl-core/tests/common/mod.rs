//! Shared fixtures for murl-core integration tests: a hermetic environment
//! (temp store/cache, fixed clock, in-memory trust), a mock remote fetcher,
//! and a recording launcher.
//!
//! Each integration-test binary compiles this module independently, so not
//! every helper is used by every binary.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{json, Value};

use murl_core::cache::ManifestCache;
use murl_core::dispatch::Launcher;
use murl_core::error::{Error, Result};
use murl_core::fetch::{LocalStore, RemoteFetcher};
use murl_core::limits::Limits;
use murl_core::murl::Murl;
use murl_core::resolver::Resolver;
use murl_core::time::FixedClock;
use murl_core::trust::TrustStore;

pub struct Env {
    pub root: PathBuf,
    pub store: LocalStore,
    pub cache: ManifestCache,
    pub trust: TrustStore,
    pub clock: FixedClock,
    pub limits: Limits,
}

impl Env {
    pub fn new(tag: &str) -> Env {
        let root = std::env::temp_dir().join(format!(
            "murl-it-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        Env {
            store: LocalStore::new(root.join("names")),
            cache: ManifestCache::new(root.join("cache")),
            trust: TrustStore::in_memory(),
            clock: FixedClock(1_700_000_000),
            limits: Limits::default(),
            root,
        }
    }

    pub fn resolver<'a>(&'a self, remote: Option<&'a dyn RemoteFetcher>) -> Resolver<'a> {
        Resolver {
            local_store: &self.store,
            remote,
            cache: Some(&self.cache),
            trust_store: &self.trust,
            limits: self.limits.clone(),
            clock: &self.clock,
        }
    }

    pub fn add_local(&self, murl: &str, bytes: &[u8]) {
        self.store.add(&Murl::parse(murl).unwrap(), bytes).unwrap();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

/// Build a minimal manifest document.
pub fn manifest(name: &str, resources: Value) -> Value {
    json!({"murlVersion": "0.1", "name": name, "resources": resources})
}

pub fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}

pub fn res(id: &str, kind: &str, target: &str) -> Value {
    json!({"id": id, "kind": kind, "target": target})
}

#[derive(Debug, Default)]
pub struct MockFetcher {
    pub responses: HashMap<String, Vec<u8>>,
    pub fail_all: bool,
    pub calls: RefCell<Vec<String>>,
}

impl MockFetcher {
    pub fn with(url: &str, body: Vec<u8>) -> MockFetcher {
        let mut f = MockFetcher::default();
        f.responses.insert(url.to_owned(), body);
        f
    }

    pub fn failing() -> MockFetcher {
        MockFetcher {
            fail_all: true,
            ..MockFetcher::default()
        }
    }
}

impl RemoteFetcher for MockFetcher {
    fn fetch(&self, url: &str, _limits: &Limits) -> Result<Vec<u8>> {
        self.calls.borrow_mut().push(url.to_owned());
        if self.fail_all {
            return Err(Error::Fetch(format!("simulated network failure for {url}")));
        }
        self.responses
            .get(url)
            .cloned()
            .ok_or_else(|| Error::Fetch(format!("404 for {url}")))
    }
}

#[derive(Debug, Default)]
pub struct RecordingLauncher {
    pub launched: RefCell<Vec<(Vec<String>, Option<PathBuf>)>>,
    pub sleeps: RefCell<Vec<u64>>,
    pub fail_program: Option<String>,
}

impl Launcher for RecordingLauncher {
    fn launch(&self, argv: &[String], cwd: Option<&std::path::Path>) -> Result<()> {
        if self.fail_program.as_deref() == Some(argv[0].as_str()) {
            return Err(Error::Dispatch("simulated launch failure".into()));
        }
        self.launched
            .borrow_mut()
            .push((argv.to_vec(), cwd.map(|p| p.to_path_buf())));
        Ok(())
    }

    fn path_exists(&self, path: &std::path::Path) -> bool {
        path.exists()
    }

    fn sleep_ms(&self, ms: u64) {
        self.sleeps.borrow_mut().push(ms);
    }
}
