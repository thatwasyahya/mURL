//! Integration tests for the dispatch engine: argv construction, handler
//! wiring, failure semantics, and aggregate status — all through the
//! recording launcher, so nothing actually opens.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use common::*;
use serde_json::json;

use murl_core::dispatch::{execute, AggregateStatus, Approval, OpenerConfig, OutcomeStatus};
use murl_core::murl::Murl;
use murl_core::resolver::Resolution;

fn opener() -> OpenerConfig {
    OpenerConfig {
        open_argv: vec!["xdg-open".into()],
        terminal_argv: None,
        ssh_argv: None,
        remote_desktop_argv: None,
        custom: BTreeMap::new(),
        home_dir: Some(PathBuf::from("/home/u")),
    }
}

fn resolve(env: &Env, murl: &str) -> Resolution {
    env.resolver(None)
        .resolve(&Murl::parse(murl).unwrap())
        .unwrap()
}

#[test]
fn https_dispatch_builds_opener_argv() {
    let env = Env::new("d-https");
    let m = manifest("P", json!([res("web", "https", "https://example.com/x")]));
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let report = execute(&r, &[Approval::Approved], &opener(), &launcher, &env.limits).unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Success);
    let launched = launcher.launched.borrow();
    assert_eq!(launched.len(), 1);
    assert_eq!(
        launched[0].0,
        vec!["xdg-open".to_string(), "https://example.com/x".to_string()]
    );
}

#[test]
fn missing_file_is_unavailable_not_failed_launch() {
    let env = Env::new("d-missing");
    let m = manifest("P", json!([res("f", "file", "/definitely/not/here.txt")]));
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let report = execute(&r, &[Approval::Approved], &opener(), &launcher, &env.limits).unwrap();
    assert_eq!(report.outcomes[0].status, OutcomeStatus::Unavailable);
    assert_eq!(report.aggregate, AggregateStatus::Failed);
    assert!(launcher.launched.borrow().is_empty());
}

#[test]
fn existing_file_dispatches_with_expanded_path() {
    let env = Env::new("d-file");
    let file = env.root.join("doc.txt");
    std::fs::write(&file, "hi").unwrap();
    let m = manifest("P", json!([res("f", "file", file.to_str().unwrap())]));
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let report = execute(&r, &[Approval::Approved], &opener(), &launcher, &env.limits).unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Success);
    let launched = launcher.launched.borrow();
    assert_eq!(launched[0].0[1], file.to_string_lossy());
}

#[test]
fn terminal_requires_a_configured_handler() {
    let env = Env::new("d-term");
    let dir = env.root.join("work");
    std::fs::create_dir_all(&dir).unwrap();
    let m = manifest("P", json!([res("t", "terminal", dir.to_str().unwrap())]));
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    // Without a handler: failed, nothing launched.
    let launcher = RecordingLauncher::default();
    let report = execute(&r, &[Approval::Approved], &opener(), &launcher, &env.limits).unwrap();
    assert_eq!(report.outcomes[0].status, OutcomeStatus::Failed);
    assert!(launcher.launched.borrow().is_empty());

    // With a handler: substituted argv, cwd set.
    let mut cfg = opener();
    cfg.terminal_argv = Some(vec!["myterm".into(), "--cwd={target}".into()]);
    let launcher = RecordingLauncher::default();
    let report = execute(&r, &[Approval::Approved], &cfg, &launcher, &env.limits).unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Success);
    let launched = launcher.launched.borrow();
    assert_eq!(launched[0].0[0], "myterm");
    assert!(launched[0].0[1].starts_with("--cwd=") && launched[0].0[1].contains("work"));
    assert_eq!(launched[0].1.as_deref(), Some(dir.as_path()));
}

#[test]
fn custom_kind_requires_registration() {
    let env = Env::new("d-custom");
    let m = manifest(
        "P",
        json!([res("v", "custom:vscode", "https://vscode.dev/x")]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let report = execute(&r, &[Approval::Approved], &opener(), &launcher, &env.limits).unwrap();
    assert_eq!(report.outcomes[0].status, OutcomeStatus::Failed);

    let mut cfg = opener();
    cfg.custom.insert(
        "vscode".into(),
        vec!["code".into(), "--open-url".into(), "{target}".into()],
    );
    let launcher = RecordingLauncher::default();
    let report = execute(&r, &[Approval::Approved], &cfg, &launcher, &env.limits).unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Success);
    assert_eq!(
        launcher.launched.borrow()[0].0,
        vec![
            "code".to_string(),
            "--open-url".to_string(),
            "https://vscode.dev/x".to_string()
        ]
    );
}

#[test]
fn ssh_and_remote_desktop_require_configured_handlers() {
    let env = Env::new("d-remote");
    let m = manifest(
        "P",
        json!([
            res("shell", "ssh", "ssh://deploy@host.example"),
            res("desk", "remote-desktop", "rdp://desktop.example"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    // Unconfigured: both fail, nothing launches.
    let launcher = RecordingLauncher::default();
    let approvals = vec![Approval::Approved; 2];
    let report = execute(&r, &approvals, &opener(), &launcher, &env.limits).unwrap();
    assert!(
        launcher.launched.borrow().is_empty(),
        "{:?}",
        launcher.launched
    );
    assert!(report
        .outcomes
        .iter()
        .all(|o| o.status == OutcomeStatus::Failed));

    // Configured: the target is substituted into a single argv element, so
    // a hostile-looking target can never become extra arguments.
    let mut cfg = opener();
    cfg.ssh_argv = Some(vec![
        "term".into(),
        "-e".into(),
        "ssh".into(),
        "{target}".into(),
    ]);
    cfg.remote_desktop_argv = Some(vec!["xfreerdp".into(), "{target}".into()]);
    let launcher = RecordingLauncher::default();
    let report = execute(&r, &approvals, &cfg, &launcher, &env.limits).unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Success);
    let launched = launcher.launched.borrow();
    assert_eq!(launched[0].0[3], "ssh://deploy@host.example");
    assert_eq!(
        launched[1].0,
        vec!["xfreerdp".to_string(), "rdp://desktop.example".to_string()]
    );
}

#[test]
fn geo_and_mailto_go_to_the_platform_opener() {
    let env = Env::new("d-uri");
    let m = manifest(
        "P",
        json!([
            res("where", "geo", "geo:48.8584,2.2945"),
            res("mail", "mailto", "mailto:team@example.com?subject=Incident"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let approvals = vec![Approval::Approved; 2];
    let report = execute(&r, &approvals, &opener(), &launcher, &env.limits).unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Success);
    let launched = launcher.launched.borrow();
    assert_eq!(
        launched[0].0,
        vec!["xdg-open".to_string(), "geo:48.8584,2.2945".to_string()]
    );
    assert_eq!(launched[1].0[1], "mailto:team@example.com?subject=Incident");
}

#[test]
fn denied_resources_do_not_launch_and_mixed_results_are_partial() {
    let env = Env::new("d-mixed");
    let m = manifest(
        "P",
        json!([
            res("a", "https", "https://e.com/a"),
            res("b", "https", "https://e.com/b"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let report = execute(
        &r,
        &[Approval::Approved, Approval::Denied("policy".into())],
        &opener(),
        &launcher,
        &env.limits,
    )
    .unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Partial);
    assert_eq!(launcher.launched.borrow().len(), 1);
    assert_eq!(report.outcomes[1].status, OutcomeStatus::Denied);
}

#[test]
fn required_resource_failure_fails_the_activation() {
    let env = Env::new("d-required");
    let m = manifest(
        "P",
        json!([
            res("a", "https", "https://e.com/a"),
            {"id": "must", "kind": "file", "target": "/nope", "required": true},
        ]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let report = execute(
        &r,
        &[Approval::Approved, Approval::Approved],
        &opener(),
        &launcher,
        &env.limits,
    )
    .unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Failed);
}

#[test]
fn all_denied_is_denied_and_launches_nothing() {
    let env = Env::new("d-denied");
    let m = manifest("P", json!([res("a", "https", "https://e.com/a")]));
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let report = execute(
        &r,
        &[Approval::Denied("no".into())],
        &opener(),
        &launcher,
        &env.limits,
    )
    .unwrap();
    assert_eq!(report.aggregate, AggregateStatus::Denied);
    assert!(launcher.launched.borrow().is_empty());
}

#[test]
fn launches_are_staggered_sequentially() {
    let env = Env::new("d-stagger");
    let m = manifest(
        "P",
        json!([
            res("a", "https", "https://e.com/a"),
            res("b", "https", "https://e.com/b"),
            res("c", "https", "https://e.com/c"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let approvals = vec![Approval::Approved; 3];
    execute(&r, &approvals, &opener(), &launcher, &env.limits).unwrap();
    // Two gaps for three launches, each the configured stagger.
    assert_eq!(*launcher.sleeps.borrow(), vec![150, 150]);
}

#[test]
fn launch_failure_is_reported_per_resource() {
    let env = Env::new("d-fail");
    let m = manifest("P", json!([res("a", "https", "https://e.com/a")]));
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher {
        fail_program: Some("xdg-open".into()),
        ..Default::default()
    };
    let report = execute(&r, &[Approval::Approved], &opener(), &launcher, &env.limits).unwrap();
    assert_eq!(report.outcomes[0].status, OutcomeStatus::Failed);
    assert_eq!(report.aggregate, AggregateStatus::Failed);
}

#[test]
fn approval_count_mismatch_is_an_error() {
    let env = Env::new("d-mismatch");
    let m = manifest("P", json!([res("a", "https", "https://e.com/a")]));
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");
    let launcher = RecordingLauncher::default();
    assert!(execute(&r, &[], &opener(), &launcher, &env.limits).is_err());
}

/// Adversarial-review hardening: `Path::join` replaces the base on an
/// absolute argument, so `~//etc/passwd` used to expand to `/etc/passwd`
/// while the plan displayed a path under the home directory.
#[test]
fn tilde_paths_cannot_escape_the_home_directory() {
    let env = Env::new("d-tilde");
    let m = manifest(
        "P",
        json!([
            res("escape", "file", "~//etc/passwd"),
            res("normal", "file", "~/notes.txt"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    let launcher = RecordingLauncher::default();
    let approvals = vec![Approval::Approved; 2];
    let report = execute(&r, &approvals, &opener(), &launcher, &env.limits).unwrap();

    let escape = &report.outcomes[0];
    assert_eq!(escape.status, OutcomeStatus::Failed, "{escape:?}");
    assert!(
        escape.detail.as_deref().unwrap_or("").contains("escape"),
        "{escape:?}"
    );
    // Nothing under /etc was ever handed to a launcher.
    for (argv, _) in launcher.launched.borrow().iter() {
        assert!(!argv.iter().any(|a| a.contains("/etc/passwd")), "{argv:?}");
    }
}

/// Adversarial-review hardening: an empty handler template would put the
/// resource target into argv[0] — that is, execute the target as a program.
#[test]
fn empty_handler_templates_never_execute_the_target() {
    let env = Env::new("d-empty-argv");
    let m = manifest(
        "P",
        json!([
            res("web", "https", "https://example.com/x"),
            res("v", "custom:thing", "/usr/bin/whatever"),
        ]),
    );
    env.add_local("murl://local/p", &bytes(&m));
    let r = resolve(&env, "murl://local/p");

    // The shapes a hand-edited handlers.json can take.
    let mut cfg = opener();
    cfg.open_argv = Vec::new();
    cfg.custom.insert("thing".into(), Vec::new());

    let launcher = RecordingLauncher::default();
    let approvals = vec![Approval::Approved; 2];
    let report = execute(&r, &approvals, &cfg, &launcher, &env.limits).unwrap();
    assert!(
        launcher.launched.borrow().is_empty(),
        "nothing may launch: {:?}",
        launcher.launched
    );
    assert!(report
        .outcomes
        .iter()
        .all(|o| o.status == OutcomeStatus::Failed));
}
