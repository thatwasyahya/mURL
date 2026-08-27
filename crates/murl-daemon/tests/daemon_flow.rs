//! Integration tests for the daemon, centered on the properties that make
//! a local "open this" socket safe (docs/daemon.md, threats D-1 … D-7).
//!
//! Everything runs through `serve_connection` over in-memory buffers: the
//! full resolve → policy → consent → dispatch path, with no socket and no
//! real launching.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use serde_json::{json, Value};

use murl_core::cache::ManifestCache;
use murl_core::dispatch::{Launcher, OpenerConfig};
use murl_core::error::Result;
use murl_core::fetch::LocalStore;
use murl_core::murl::Murl;
use murl_core::policy::Policy;
use murl_core::resolver::Resolver;
use murl_core::time::FixedClock;
use murl_core::trust::TrustStore;
use murl_core::Limits;

use murl_daemon::consent_ui::{ConsentRequest, ConsentUi};
use murl_daemon::protocol::PROTOCOL_VERSION;
use murl_daemon::server::{serve_connection, Context};

// ---------------------------------------------------------------- fixtures

#[derive(Debug, Default)]
struct RecordingLauncher {
    launched: RefCell<Vec<Vec<String>>>,
}

impl Launcher for RecordingLauncher {
    fn launch(&self, argv: &[String], _cwd: Option<&Path>) -> Result<()> {
        self.launched.borrow_mut().push(argv.to_vec());
        Ok(())
    }
    fn path_exists(&self, _path: &Path) -> bool {
        true
    }
    fn sleep_ms(&self, _ms: u64) {}
}

#[derive(Debug)]
struct GrantAllUi;
impl ConsentUi for GrantAllUi {
    fn ask(&self, request: &ConsentRequest) -> Vec<usize> {
        request.items.iter().map(|i| i.index).collect()
    }
}

#[derive(Debug)]
struct GrantEverythingUi;
impl ConsentUi for GrantEverythingUi {
    fn ask(&self, _request: &ConsentRequest) -> Vec<usize> {
        // A rogue surface claiming every index, offered or not.
        (0..64).collect()
    }
}

#[derive(Debug)]
struct DenyUi;
impl ConsentUi for DenyUi {
    fn ask(&self, _request: &ConsentRequest) -> Vec<usize> {
        Vec::new()
    }
}

struct Env {
    root: PathBuf,
    store: LocalStore,
    cache: ManifestCache,
    trust: TrustStore,
}

impl Env {
    fn new(tag: &str) -> Env {
        let root = std::env::temp_dir().join(format!(
            "murl-daemon-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        Env {
            store: LocalStore::new(root.join("names")),
            cache: ManifestCache::new(root.join("cache")),
            trust: TrustStore::in_memory(),
            root,
        }
    }

    fn add(&self, name: &str, doc: &Value) {
        self.store
            .add(
                &Murl::parse(name).unwrap(),
                &serde_json::to_vec(doc).unwrap(),
            )
            .unwrap();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

/// Run request lines through a fully wired daemon; return response values
/// and the argv of everything launched.
fn exchange(
    env: &Env,
    consent: &dyn ConsentUi,
    requests: &[Value],
) -> (Vec<Value>, Vec<Vec<String>>) {
    let limits = Limits::default();
    let clock = FixedClock(1_700_000_000);
    let launcher = RecordingLauncher::default();
    let opener = OpenerConfig {
        open_argv: vec!["stub-open".into()],
        terminal_argv: Some(vec!["stub-term".into(), "{target}".into()]),
        custom: Default::default(),
        home_dir: Some(env.root.clone()),
    };

    let resolver_limits = limits.clone();
    let with_resolver = |f: &mut dyn FnMut(&Resolver<'_>) -> Result<()>| -> Result<()> {
        let resolver = Resolver {
            local_store: &env.store,
            remote: None,
            cache: Some(&env.cache),
            trust_store: &env.trust,
            limits: resolver_limits.clone(),
            clock: &clock,
        };
        f(&resolver)
    };

    let ctx = Context {
        with_resolver: &with_resolver,
        policy: Policy::default(),
        opener,
        launcher: &launcher,
        consent,
        limits,
        started_at: 1_699_999_000,
        socket: "test".into(),
        activations: AtomicU64::new(0),
        version: "test",
    };

    let input: String = requests
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<Vec<_>>()
        .join("");
    let mut output: Vec<u8> = Vec::new();
    serve_connection(&ctx, input.as_bytes(), &mut output, 1_700_000_500, 32).unwrap();

    let responses = String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("response is JSON"))
        .collect();
    let launched = launcher.launched.borrow().clone();
    (responses, launched)
}

fn safe_manifest() -> Value {
    json!({
        "murlVersion": "0.2",
        "name": "Project",
        "resources": [
            {"id": "docs", "kind": "https", "target": "https://docs.example/x"},
            {"id": "site", "kind": "https", "target": "https://site.example/x"}
        ]
    })
}

// ------------------------------------------------------------------- tests

#[test]
fn ping_reports_protocol_and_version() {
    let env = Env::new("ping");
    let (responses, launched) = exchange(
        &env,
        &DenyUi,
        &[json!({"type": "ping", "protocol": PROTOCOL_VERSION})],
    );
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "pong");
    assert_eq!(responses[0]["protocol"], PROTOCOL_VERSION);
    assert!(launched.is_empty());
}

#[test]
fn resolve_returns_a_plan_and_opens_nothing() {
    let env = Env::new("resolve");
    env.add("murl://local/p", &safe_manifest());
    let (responses, launched) = exchange(
        &env,
        &GrantAllUi,
        &[json!({"type": "resolve", "protocol": PROTOCOL_VERSION, "murl": "murl://local/p"})],
    );
    assert_eq!(responses[0]["type"], "plan");
    assert_eq!(
        responses[0]["resolution"]["resources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        launched.is_empty(),
        "resolve must never dispatch: {launched:?}"
    );
}

#[test]
fn activate_asks_consent_then_dispatches() {
    let env = Env::new("activate");
    env.add("murl://local/p", &safe_manifest());
    let (responses, launched) = exchange(
        &env,
        &GrantAllUi,
        &[json!({"type": "activate", "protocol": PROTOCOL_VERSION, "murl": "murl://local/p"})],
    );
    let types: Vec<&str> = responses
        .iter()
        .map(|r| r["type"].as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["plan", "consent", "outcome"]);
    assert_eq!(responses[2]["report"]["aggregate"], "SUCCESS");
    assert_eq!(launched.len(), 2);
    assert_eq!(launched[0][0], "stub-open");
}

#[test]
fn refusing_consent_launches_nothing() {
    let env = Env::new("deny");
    env.add("murl://local/p", &safe_manifest());
    let (responses, launched) = exchange(
        &env,
        &DenyUi,
        &[json!({"type": "activate", "protocol": PROTOCOL_VERSION, "murl": "murl://local/p"})],
    );
    assert!(launched.is_empty(), "{launched:?}");
    assert_eq!(responses[2]["report"]["aggregate"], "DENIED");
    let denied = responses[1]["denied"].as_array().unwrap();
    assert_eq!(denied.len(), 2);
}

#[test]
fn only_narrows_and_cannot_widen() {
    let env = Env::new("only");
    env.add("murl://local/p", &safe_manifest());
    let (_responses, launched) = exchange(
        &env,
        &GrantAllUi,
        &[json!({
            "type": "activate", "protocol": PROTOCOL_VERSION,
            "murl": "murl://local/p", "only": ["docs"]
        })],
    );
    assert_eq!(launched.len(), 1);
    assert_eq!(launched[0][1], "https://docs.example/x");

    // An `only` naming something that isn't there selects nothing — it can
    // never add a resource the manifest did not declare.
    let (_responses, launched) = exchange(
        &env,
        &GrantAllUi,
        &[json!({
            "type": "activate", "protocol": PROTOCOL_VERSION,
            "murl": "murl://local/p", "only": ["ghost"]
        })],
    );
    assert!(launched.is_empty(), "{launched:?}");
}

#[test]
fn policy_denials_survive_a_rogue_consent_surface() {
    // A terminal from an untrusted *remote* manifest is denied by policy.
    // The manifest is reachable only via cache here, so mark it remote by
    // caching it under a remote identity.
    let env = Env::new("rogue-ui");
    let doc = json!({
        "murlVersion": "0.2",
        "name": "Remote",
        "resources": [
            {"id": "docs", "kind": "https", "target": "https://docs.example/x"},
            {"id": "term", "kind": "terminal", "target": "/tmp"}
        ]
    });
    env.cache
        .put(
            "murl://example.com/p",
            "https://example.com/.well-known/murl/p.murl.json",
            &serde_json::to_vec(&doc).unwrap(),
            1_700_000_000,
        )
        .unwrap();

    let (responses, launched) = exchange(
        &env,
        &GrantEverythingUi,
        &[
            json!({"type": "activate", "protocol": PROTOCOL_VERSION, "murl": "murl://example.com/p"}),
        ],
    );
    // The docs resource opened; the terminal did not, despite the UI
    // claiming every index.
    let programs: Vec<&str> = launched.iter().map(|argv| argv[0].as_str()).collect();
    assert!(!programs.contains(&"stub-term"), "{launched:?}");
    let denied = responses[1]["denied"].as_array().unwrap();
    assert!(
        denied.iter().any(|d| d == "term"),
        "terminal should be denied: {denied:?}"
    );
}

#[test]
fn protocol_violations_are_refused_without_side_effects() {
    let env = Env::new("protocol");
    env.add("murl://local/p", &safe_manifest());
    let (responses, launched) = exchange(
        &env,
        &GrantAllUi,
        &[
            json!({"type": "activate", "protocol": 999, "murl": "murl://local/p"}),
            json!({"type": "launch", "protocol": PROTOCOL_VERSION}),
            json!({"type": "ping", "protocol": PROTOCOL_VERSION, "extra": true}),
        ],
    );
    assert_eq!(responses.len(), 3);
    for response in &responses {
        assert_eq!(response["type"], "error", "{response}");
        assert_eq!(response["stage"], "protocol");
    }
    assert!(launched.is_empty(), "{launched:?}");
}

#[test]
fn malformed_lines_do_not_break_the_connection() {
    let env = Env::new("malformed");
    env.add("murl://local/p", &safe_manifest());
    let limits = Limits::default();
    let clock = FixedClock(1_700_000_000);
    let launcher = RecordingLauncher::default();
    let resolver_limits = limits.clone();
    let with_resolver = |f: &mut dyn FnMut(&Resolver<'_>) -> Result<()>| -> Result<()> {
        let resolver = Resolver {
            local_store: &env.store,
            remote: None,
            cache: Some(&env.cache),
            trust_store: &env.trust,
            limits: resolver_limits.clone(),
            clock: &clock,
        };
        f(&resolver)
    };
    let ctx = Context {
        with_resolver: &with_resolver,
        policy: Policy::default(),
        opener: OpenerConfig::platform_default("linux", None),
        launcher: &launcher,
        consent: &DenyUi,
        limits,
        started_at: 0,
        socket: "test".into(),
        activations: AtomicU64::new(0),
        version: "test",
    };

    // Garbage, then a valid ping: the daemon answers both, in order.
    let input = format!(
        "not json\n\n{}\n",
        json!({"type": "ping", "protocol": PROTOCOL_VERSION})
    );
    let mut output = Vec::new();
    serve_connection(&ctx, input.as_bytes(), &mut output, 0, 32).unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["type"], "error");
    assert_eq!(lines[1]["type"], "pong");
}

#[test]
fn request_count_is_bounded() {
    let env = Env::new("flood");
    let limits = Limits::default();
    let clock = FixedClock(0);
    let launcher = RecordingLauncher::default();
    let resolver_limits = limits.clone();
    let with_resolver = |f: &mut dyn FnMut(&Resolver<'_>) -> Result<()>| -> Result<()> {
        let resolver = Resolver {
            local_store: &env.store,
            remote: None,
            cache: Some(&env.cache),
            trust_store: &env.trust,
            limits: resolver_limits.clone(),
            clock: &clock,
        };
        f(&resolver)
    };
    let ctx = Context {
        with_resolver: &with_resolver,
        policy: Policy::default(),
        opener: OpenerConfig::platform_default("linux", None),
        launcher: &launcher,
        consent: &DenyUi,
        limits,
        started_at: 0,
        socket: "test".into(),
        activations: AtomicU64::new(0),
        version: "test",
    };

    let ping = format!(
        "{}\n",
        json!({"type": "ping", "protocol": PROTOCOL_VERSION})
    );
    let input = ping.repeat(100);
    let mut output = Vec::new();
    serve_connection(&ctx, input.as_bytes(), &mut output, 0, 5).unwrap();
    let count = String::from_utf8(output).unwrap().lines().count();
    assert_eq!(count, 5, "connection must stop at max_requests");
}

#[test]
fn unresolvable_names_report_errors_not_panics() {
    let env = Env::new("errors");
    let (responses, launched) = exchange(
        &env,
        &GrantAllUi,
        &[
            json!({"type": "resolve", "protocol": PROTOCOL_VERSION, "murl": "murl://local/ghost"}),
            json!({"type": "activate", "protocol": PROTOCOL_VERSION, "murl": "not-a-murl"}),
        ],
    );
    assert_eq!(responses[0]["type"], "error");
    assert_eq!(responses[0]["stage"], "resolve");
    assert_eq!(responses[1]["type"], "error");
    assert_eq!(responses[1]["stage"], "parse");
    assert!(launched.is_empty());
}

#[test]
fn status_reports_activations() {
    let env = Env::new("status");
    env.add("murl://local/p", &safe_manifest());
    let (responses, _launched) = exchange(
        &env,
        &GrantAllUi,
        &[
            json!({"type": "activate", "protocol": PROTOCOL_VERSION, "murl": "murl://local/p"}),
            json!({"type": "status", "protocol": PROTOCOL_VERSION}),
        ],
    );
    let status = responses.last().unwrap();
    assert_eq!(status["type"], "status");
    assert_eq!(status["activations"], 1);
    assert_eq!(status["uptime_secs"], 1500);
}
