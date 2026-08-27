//! Integration tests for the resolver, centered on its security guarantees:
//! recursion limits, cycle detection, identity binding, integrity pins,
//! trust states, cache fallback, and policy outcomes.

mod common;

use common::*;
use serde_json::json;

use murl_core::error::Error;
use murl_core::murl::Murl;
use murl_core::policy::{Decision, Policy, Tier};
use murl_core::trust::{make_integrity, sign_manifest, Keypair, TrustStatus};

fn murl(s: &str) -> Murl {
    Murl::parse(s).unwrap()
}

#[test]
fn local_resolution_end_to_end() {
    let env = Env::new("local-e2e");
    let m = manifest(
        "Project X",
        json!([
            res("source", "https", "https://github.com/acme/project-x"),
            res("docs", "https", "https://docs.acme.example/project-x"),
        ]),
    );
    env.add_local("murl://local/project-x", &bytes(&m));

    let r = env
        .resolver(None)
        .resolve(&murl("murl://local/project-x"))
        .unwrap();
    assert_eq!(r.nodes.len(), 1);
    assert_eq!(r.resources.len(), 2);
    assert_eq!(r.nodes[0].trust, TrustStatus::Local);
    assert!(!r.nodes[0].origin.is_remote());
    assert!(r.warnings.is_empty());
}

#[test]
fn missing_local_name_is_not_found() {
    let env = Env::new("missing");
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/ghost"))
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err}");
}

#[test]
fn nested_murls_are_spliced_with_anchors() {
    let env = Env::new("nested");
    let child = manifest(
        "Team",
        json!([res("wiki", "https", "https://wiki.example/team")]),
    );
    env.add_local("murl://local/team", &bytes(&child));
    let parent = manifest(
        "Project",
        json!([
            res("source", "https", "https://github.com/acme/p"),
            res("team", "murl", "murl://local/team"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&parent));

    let r = env.resolver(None).resolve(&murl("murl://local/p")).unwrap();
    assert_eq!(r.nodes.len(), 2);
    assert_eq!(r.resources.len(), 2); // source + wiki (the murl container is not dispatchable)
    let wiki = r
        .resources
        .iter()
        .find(|p| p.resource.id == "wiki")
        .unwrap();
    assert_eq!(wiki.root_anchor, "team");
    assert_eq!(r.nodes[wiki.node].depth, 1);
}

#[test]
fn cycles_are_hard_errors() {
    let env = Env::new("cycle");
    let a = manifest("A", json!([res("b", "murl", "murl://local/b")]));
    let b = manifest("B", json!([res("a", "murl", "murl://local/a")]));
    env.add_local("murl://local/a", &bytes(&a));
    env.add_local("murl://local/b", &bytes(&b));

    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/a"))
        .unwrap_err();
    assert!(matches!(err, Error::Cycle(_)), "{err}");
    let msg = err.to_string();
    assert!(
        msg.contains("murl://local/a") && msg.contains("murl://local/b"),
        "{msg}"
    );
}

#[test]
fn self_referencing_murl_is_a_cycle() {
    let env = Env::new("self-cycle");
    let a = manifest("A", json!([res("me", "murl", "murl://local/a")]));
    env.add_local("murl://local/a", &bytes(&a));
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/a"))
        .unwrap_err();
    assert!(matches!(err, Error::Cycle(_)), "{err}");
}

#[test]
fn depth_limit_is_enforced() {
    let mut env = Env::new("depth");
    env.limits.max_depth = 1;
    let a = manifest("A", json!([res("b", "murl", "murl://local/b")]));
    let b = manifest("B", json!([res("c", "murl", "murl://local/c")]));
    let c = manifest("C", json!([res("x", "https", "https://e.com/x")]));
    env.add_local("murl://local/a", &bytes(&a));
    env.add_local("murl://local/b", &bytes(&b));
    env.add_local("murl://local/c", &bytes(&c));

    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/a"))
        .unwrap_err();
    assert!(matches!(err, Error::LimitExceeded(_)), "{err}");
}

#[test]
fn total_resource_limit_is_enforced() {
    let mut env = Env::new("count");
    env.limits.max_total_resources = 2;
    let m = manifest(
        "Big",
        json!([
            res("a", "https", "https://e.com/1"),
            res("b", "https", "https://e.com/2"),
            res("c", "https", "https://e.com/3"),
        ]),
    );
    env.add_local("murl://local/big", &bytes(&m));
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/big"))
        .unwrap_err();
    assert!(matches!(err, Error::LimitExceeded(_)), "{err}");
}

#[test]
fn duplicate_targets_are_skipped_with_warning() {
    let env = Env::new("dedup");
    let child = manifest(
        "Child",
        json!([res("same", "https", "https://e.com/shared")]),
    );
    env.add_local("murl://local/child", &bytes(&child));
    let parent = manifest(
        "Parent",
        json!([
            res("mine", "https", "https://e.com/shared"),
            res("child", "murl", "murl://local/child"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&parent));

    let r = env.resolver(None).resolve(&murl("murl://local/p")).unwrap();
    assert_eq!(r.resources.len(), 1);
    assert!(
        r.warnings.iter().any(|w| w.contains("duplicate resource")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn selector_filters_to_one_root_resource() {
    let env = Env::new("selector");
    let team = manifest(
        "Team",
        json!([res("wiki", "https", "https://wiki.example/x")]),
    );
    env.add_local("murl://local/team", &bytes(&team));
    let p = manifest(
        "P",
        json!([
            res("docs", "https", "https://docs.example/p"),
            res("team", "murl", "murl://local/team"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&p));

    // Selecting a plain resource.
    let r = env
        .resolver(None)
        .resolve(&murl("murl://local/p#docs"))
        .unwrap();
    assert_eq!(r.resources.len(), 1);
    assert_eq!(r.resources[0].resource.id, "docs");

    // Selecting a murl container keeps its spliced children.
    let r = env
        .resolver(None)
        .resolve(&murl("murl://local/p#team"))
        .unwrap();
    assert_eq!(r.resources.len(), 1);
    assert_eq!(r.resources[0].resource.id, "wiki");

    // Unknown selector is an error, not an empty success.
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/p#ghost"))
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err}");
}

#[test]
fn selector_role_tag_and_multi_items() {
    let env = Env::new("selector-v2");
    let m = serde_json::json!({
        "murlVersion": "0.1",
        "name": "P",
        "resources": [
            {"id": "src", "kind": "https", "target": "https://e.com/src", "role": "source"},
            {"id": "docs", "kind": "https", "target": "https://e.com/docs", "role": "docs"},
            {"id": "wiki", "kind": "https", "target": "https://e.com/wiki", "role": "docs",
             "tags": ["team"]},
            {"id": "dash", "kind": "https", "target": "https://e.com/dash", "tags": ["ops"]},
        ]
    });
    env.add_local("murl://local/p", &bytes(&m));

    // role= matches every resource with that role.
    let r = env
        .resolver(None)
        .resolve(&murl("murl://local/p#role=docs"))
        .unwrap();
    let ids: Vec<&str> = r.resources.iter().map(|p| p.resource.id.as_str()).collect();
    assert_eq!(ids, vec!["docs", "wiki"]);

    // tag= matches tag carriers.
    let r = env
        .resolver(None)
        .resolve(&murl("murl://local/p#tag=ops"))
        .unwrap();
    assert_eq!(r.resources.len(), 1);
    assert_eq!(r.resources[0].resource.id, "dash");

    // Multi-item union, deduplicated by the retain pass.
    let r = env
        .resolver(None)
        .resolve(&murl("murl://local/p#src,role=docs"))
        .unwrap();
    let ids: Vec<&str> = r.resources.iter().map(|p| p.resource.id.as_str()).collect();
    assert_eq!(ids, vec!["src", "docs", "wiki"]);

    // Every item must match: one dead item fails the whole selector.
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/p#src,role=nope"))
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err}");
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/p#tag=nope"))
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err}");
}

#[test]
fn premature_manifest_blocks_sensitive_resources() {
    let env = Env::new("premature");
    let mut m = manifest(
        "P",
        json!([
            res("web", "https", "https://e.com"),
            res("notes", "file", "/home/u/notes.txt"),
        ]),
    );
    m["notBefore"] = json!("2030-01-01T00:00:00Z"); // clock is 2023
    env.add_local("murl://local/p", &bytes(&m));
    let mut r = env.resolver(None).resolve(&murl("murl://local/p")).unwrap();
    assert!(r.nodes[0].premature);
    assert!(
        r.warnings.iter().any(|w| w.contains("not valid before")),
        "{:?}",
        r.warnings
    );
    r.apply_policy(&Policy::default());
    let notes = r
        .resources
        .iter()
        .find(|p| p.resource.id == "notes")
        .unwrap();
    assert!(matches!(notes.decision, Some(Decision::Deny(_))));
    let web = r.resources.iter().find(|p| p.resource.id == "web").unwrap();
    assert!(matches!(web.decision, Some(Decision::Prompt(_))));
}

#[test]
fn duplicate_manifest_members_fail_resolution() {
    let env = Env::new("dup-members");
    env.add_local(
        "murl://local/dup",
        br#"{"murlVersion":"0.1","name":"D","resources":[
             {"id":"a","kind":"https","target":"https://safe.example",
              "target":"https://evil.example"}]}"#,
    );
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/dup"))
        .unwrap_err();
    assert!(err.to_string().contains("duplicate"), "{err}");
}

#[test]
fn identity_binding_rejects_relabelled_manifests() {
    let env = Env::new("binding");
    let mut m = manifest("M", json!([res("a", "https", "https://e.com")]));
    m["id"] = json!("murl://local/other-name");
    env.add_local("murl://local/mine", &bytes(&m));

    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/mine"))
        .unwrap_err();
    assert!(matches!(err, Error::Resolution(_)), "{err}");
    assert!(err.to_string().contains("re-labeled"), "{err}");
}

#[test]
fn integrity_pins_on_nested_murls() {
    let env = Env::new("integrity");
    let child = manifest("Child", json!([res("w", "https", "https://e.com/w")]));
    let child_bytes = bytes(&child);
    env.add_local("murl://local/child", &child_bytes);

    // Correct pin resolves.
    let good = manifest(
        "P",
        json!([{
            "id": "child", "kind": "murl", "target": "murl://local/child",
            "integrity": make_integrity(&child_bytes)
        }]),
    );
    env.add_local("murl://local/good", &bytes(&good));
    assert!(env
        .resolver(None)
        .resolve(&murl("murl://local/good"))
        .is_ok());

    // Wrong pin is a trust error.
    let bad = manifest(
        "P",
        json!([{
            "id": "child", "kind": "murl", "target": "murl://local/child",
            "integrity": make_integrity(b"something else")
        }]),
    );
    env.add_local("murl://local/bad", &bytes(&bad));
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/bad"))
        .unwrap_err();
    assert!(matches!(err, Error::Trust(_)), "{err}");
}

#[test]
fn remote_resolution_trust_ladder() {
    let env = Env::new("trust-ladder");
    let url = "https://example.com/.well-known/murl/p.murl.json";

    // Unsigned remote.
    let unsigned = manifest("P", json!([res("a", "https", "https://e.com")]));
    let fetcher = MockFetcher::with(url, bytes(&unsigned));
    let r = env
        .resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert_eq!(r.nodes[0].trust, TrustStatus::UnsignedRemote);

    // Signed by an unknown key.
    let kp = Keypair::generate().unwrap();
    let mut signed = manifest("P", json!([res("a", "https", "https://e.com")]));
    signed["id"] = json!("murl://example.com/p");
    sign_manifest(&mut signed, &kp).unwrap();
    let env2 = Env::new("trust-ladder2");
    let fetcher = MockFetcher::with(url, bytes(&signed));
    let r = env2
        .resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert_eq!(
        r.nodes[0].trust,
        TrustStatus::SignedUnknownKey {
            key_id: kp.key_id()
        }
    );

    // Same manifest after pinning the key: trusted.
    let mut env3 = Env::new("trust-ladder3");
    env3.trust
        .add("example.com", &kp.public_key_b64(), 0)
        .unwrap();
    let fetcher = MockFetcher::with(url, bytes(&signed));
    let r = env3
        .resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert_eq!(
        r.nodes[0].trust,
        TrustStatus::SignedTrusted {
            key_id: kp.key_id()
        }
    );
}

#[test]
fn tampered_signed_manifest_is_a_hard_stop() {
    let env = Env::new("tampered");
    let kp = Keypair::generate().unwrap();
    let mut signed = manifest("P", json!([res("a", "https", "https://e.com")]));
    sign_manifest(&mut signed, &kp).unwrap();
    signed["name"] = json!("Tampered");
    let url = "https://example.com/.well-known/murl/p.murl.json";
    let fetcher = MockFetcher::with(url, bytes(&signed));
    let err = env
        .resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap_err();
    assert!(matches!(err, Error::Trust(_)), "{err}");
}

#[test]
fn policy_denies_dangerous_from_untrusted_remote() {
    let env = Env::new("policy-dangerous");
    let m = manifest(
        "P",
        json!([
            res("web", "https", "https://e.com"),
            res("term", "terminal", "/home/u/p"),
        ]),
    );
    let url = "https://example.com/.well-known/murl/p.murl.json";
    let fetcher = MockFetcher::with(url, bytes(&m));
    let mut r = env
        .resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    r.apply_policy(&Policy::default());

    let web = r.resources.iter().find(|p| p.resource.id == "web").unwrap();
    assert!(matches!(web.decision, Some(Decision::Prompt(_))));
    let term = r
        .resources
        .iter()
        .find(|p| p.resource.id == "term")
        .unwrap();
    assert_eq!(term.tier, Tier::Dangerous);
    assert!(
        matches!(term.decision, Some(Decision::Deny(_))),
        "{:?}",
        term.decision
    );
}

#[test]
fn policy_allows_dangerous_from_local_after_prompt() {
    let env = Env::new("policy-local");
    let m = manifest("P", json!([res("term", "terminal", "/home/u/p")]));
    env.add_local("murl://local/p", &bytes(&m));
    let mut r = env.resolver(None).resolve(&murl("murl://local/p")).unwrap();
    r.apply_policy(&Policy::default());
    assert!(matches!(r.resources[0].decision, Some(Decision::Prompt(_))));
}

#[test]
fn expired_manifest_blocks_sensitive_resources() {
    let env = Env::new("expired");
    let mut m = manifest(
        "P",
        json!([
            res("web", "https", "https://e.com"),
            res("notes", "file", "/home/u/notes.txt"),
        ]),
    );
    m["expires"] = json!("2001-01-01T00:00:00Z"); // clock is 2023
    env.add_local("murl://local/p", &bytes(&m));
    let mut r = env.resolver(None).resolve(&murl("murl://local/p")).unwrap();
    assert!(r.nodes[0].expired);
    r.apply_policy(&Policy::default());
    let notes = r
        .resources
        .iter()
        .find(|p| p.resource.id == "notes")
        .unwrap();
    assert!(matches!(notes.decision, Some(Decision::Deny(_))));
}

#[test]
fn cache_serves_fresh_and_falls_back_when_offline() {
    let mut env = Env::new("cache");
    let url = "https://example.com/.well-known/murl/p.murl.json";
    let m = manifest("P", json!([res("a", "https", "https://e.com")]));

    // First resolve: network hit + cache fill.
    let fetcher = MockFetcher::with(url, bytes(&m));
    env.resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert_eq!(fetcher.calls.borrow().len(), 1);

    // Second resolve within TTL: served from cache, no network call.
    let r = env
        .resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert_eq!(fetcher.calls.borrow().len(), 1);
    assert!(matches!(
        &r.nodes[0].origin,
        murl_core::resolver::Origin::Remote {
            from_cache: true,
            ..
        }
    ));

    // Offline with a *fresh* cache: silent success from cache — normal
    // operation, not a fallback.
    let r = env
        .resolver(None)
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert!(matches!(
        &r.nodes[0].origin,
        murl_core::resolver::Origin::Remote {
            from_cache: true,
            ..
        }
    ));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);

    // Offline with a *stale* cache: still resolves, but warns.
    env.clock = murl_core::time::FixedClock(1_700_000_000 + 100_000); // past TTL
    let r = env
        .resolver(None)
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert!(
        r.warnings.iter().any(|w| w.contains("offline")),
        "{:?}",
        r.warnings
    );

    // Network failing after TTL expiry: stale fallback with warning.
    let failing = MockFetcher::failing();
    let r = env
        .resolver(Some(&failing))
        .resolve(&murl("murl://example.com/p"))
        .unwrap();
    assert!(
        r.warnings.iter().any(|w| w.contains("stale")),
        "{:?}",
        r.warnings
    );
    assert_eq!(failing.calls.borrow().len(), 1);
}

#[test]
fn offline_uncached_remote_fails_cleanly() {
    let env = Env::new("offline");
    let err = env
        .resolver(None)
        .resolve(&murl("murl://example.com/p"))
        .unwrap_err();
    assert!(matches!(err, Error::Resolution(_)), "{err}");
    assert!(err.to_string().contains("offline"), "{err}");
}

#[test]
fn oversized_remote_manifest_is_rejected() {
    let mut env = Env::new("oversize");
    env.limits.max_manifest_bytes = 64;
    let url = "https://example.com/.well-known/murl/p.murl.json";
    let m = manifest(
        "A name long enough to blow the tiny cap",
        json!([res("a", "https", "https://e.com")]),
    );
    let fetcher = MockFetcher::with(url, bytes(&m));
    let err = env
        .resolver(Some(&fetcher))
        .resolve(&murl("murl://example.com/p"))
        .unwrap_err();
    assert!(matches!(err, Error::LimitExceeded(_)), "{err}");
}

#[test]
fn invalid_manifest_reports_validation_error() {
    let env = Env::new("invalid");
    env.add_local(
        "murl://local/bad",
        br#"{"murlVersion":"0.1","name":"B","resources":[{"id":"UPPER","kind":"nope","target":""}]}"#,
    );
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/bad"))
        .unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "{err}");
}

#[test]
fn manifest_count_limit_bounds_fanout() {
    let mut env = Env::new("fanout");
    env.limits.max_nested_manifests = 2;
    let leaf = manifest("L", json!([res("x", "https", "https://e.com/x")]));
    env.add_local("murl://local/l1", &bytes(&leaf));
    env.add_local("murl://local/l2", &bytes(&leaf));
    let p = manifest(
        "P",
        json!([
            res("a", "murl", "murl://local/l1"),
            res("b", "murl", "murl://local/l2"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&p));
    let err = env
        .resolver(None)
        .resolve(&murl("murl://local/p"))
        .unwrap_err();
    assert!(matches!(err, Error::LimitExceeded(_)), "{err}");
}
