//! End-to-end tests of the `murl` binary.
//!
//! Hermetic by construction: every invocation points MURL_CONFIG_DIR /
//! MURL_DATA_DIR / MURL_CACHE_DIR at a per-test temp directory, dispatch is
//! rewired to a no-op program, and "remote" resolution talks to a loopback
//! HTTP server started inside the test.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};

struct TestEnv {
    root: PathBuf,
}

impl TestEnv {
    fn new(tag: &str) -> TestEnv {
        let root = std::env::temp_dir().join(format!(
            "murl-cli-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("work")).unwrap();
        TestEnv { root }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_murl"));
        c.args(args)
            .current_dir(self.root.join("work"))
            .env("MURL_CONFIG_DIR", self.root.join("config"))
            .env("MURL_DATA_DIR", self.root.join("data"))
            .env("MURL_CACHE_DIR", self.root.join("cache"))
            .env("XDG_DATA_HOME", self.root.join("xdg-data"))
            .env_remove("MURL_LOG");
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().expect("binary runs")
    }

    /// Point the generic opener at a no-op program so `open` launches
    /// nothing real.
    #[cfg(unix)]
    fn stub_opener(&self) {
        let config = self.root.join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("handlers.json"), r#"{"open": ["/bin/true"]}"#).unwrap();
    }

    fn write(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.root.join("work").join(rel);
        std::fs::write(&path, content).unwrap();
        path
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(out)).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}):\n{}\n{}",
            stdout(out),
            stderr(out)
        )
    })
}

const VALID_MANIFEST: &str = r#"{
  "murlVersion": "0.1",
  "name": "Test Project",
  "resources": [
    {"id": "docs", "kind": "https", "target": "https://example.com/docs", "role": "docs"},
    {"id": "site", "kind": "https", "target": "https://example.com/site"}
  ]
}
"#;

#[test]
fn create_validate_name_resolve_roundtrip() {
    let env = TestEnv::new("roundtrip");

    let out = env.run(&["create", "--name", "Project X"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(env.root.join("work/project-x.murl.json").exists());

    let out = env.run(&["validate", "project-x.murl.json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("OK"));

    let out = env.run(&["name", "add", "project-x", "project-x.murl.json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let out = env.run(&["name", "list"]);
    assert!(stdout(&out).contains("murl://local/project-x"));

    let out = env.run(&["--json", "resolve", "murl://local/project-x"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v = json(&out);
    assert_eq!(v["nodes"][0]["trust"]["status"], "local");
    let resources = v["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 2);
    // Default policy: everything requires consent (prompt), nothing denied.
    for r in resources {
        assert_eq!(r["decision"]["decision"], "prompt", "{r}");
    }
}

#[test]
fn parse_rejects_userinfo_with_nonzero_exit() {
    let env = TestEnv::new("parse-err");
    let out = env.run(&["parse", "murl://github.com@evil.example/x"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("userinfo"));
}

#[test]
fn validate_reports_errors_with_exit_2() {
    let env = TestEnv::new("validate-bad");
    env.write(
        "bad.murl.json",
        r#"{"murlVersion":"0.1","name":"B","resources":[{"id":"UPPER","kind":"nope","target":"x"}]}"#,
    );
    let out = env.run(&["validate", "bad.murl.json"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(stdout(&out).contains("INVALID"));
}

#[test]
fn name_add_refuses_invalid_manifest() {
    let env = TestEnv::new("name-invalid");
    env.write(
        "bad.murl.json",
        r#"{"murlVersion":"0.1","name":"B","resources":[]}"#,
    );
    let out = env.run(&["name", "add", "bad", "bad.murl.json"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    let out = env.run(&["name", "list"]);
    assert!(!stdout(&out).contains("bad"));
}

#[test]
fn name_add_refuses_id_mismatch() {
    let env = TestEnv::new("name-mismatch");
    env.write(
        "m.murl.json",
        r#"{"murlVersion":"0.1","id":"murl://local/other","name":"M",
            "resources":[{"id":"a","kind":"https","target":"https://e.com"}]}"#,
    );
    let out = env.run(&["name", "add", "mine", "m.murl.json"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("id"), "{}", stderr(&out));
}

#[cfg(unix)]
#[test]
fn open_noninteractive_denies_without_flags() {
    let env = TestEnv::new("open-deny");
    env.stub_opener();
    env.write("m.murl.json", VALID_MANIFEST);
    env.run(&["name", "add", "t", "m.murl.json"]);

    // No TTY, no flags: consent fails closed.
    let out = env.run(&["--json", "open", "murl://local/t"]);
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}\n{}",
        stdout(&out),
        stderr(&out)
    );
    let v = json(&out);
    assert_eq!(v["report"]["aggregate"], "DENIED");
}

#[cfg(unix)]
#[test]
fn open_with_yes_dispatches_safe_resources() {
    let env = TestEnv::new("open-yes");
    env.stub_opener();
    env.write("m.murl.json", VALID_MANIFEST);
    env.run(&["name", "add", "t", "m.murl.json"]);

    let out = env.run(&["--json", "open", "murl://local/t", "--yes"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}\n{}",
        stdout(&out),
        stderr(&out)
    );
    let v = json(&out);
    assert_eq!(v["report"]["aggregate"], "SUCCESS");
    let outcomes = v["report"]["outcomes"].as_array().unwrap();
    assert!(outcomes.iter().all(|o| o["status"] == "OPENED"));
}

#[cfg(unix)]
#[test]
fn open_only_and_skip_narrow_the_plan() {
    let env = TestEnv::new("open-only");
    env.stub_opener();
    env.write("m.murl.json", VALID_MANIFEST);
    env.run(&["name", "add", "t", "m.murl.json"]);

    let out = env.run(&[
        "--json",
        "open",
        "murl://local/t",
        "--yes",
        "--only",
        "docs",
    ]);
    let v = json(&out);
    let outcomes = v["report"]["outcomes"].as_array().unwrap();
    let docs = outcomes.iter().find(|o| o["id"] == "docs").unwrap();
    let site = outcomes.iter().find(|o| o["id"] == "site").unwrap();
    assert_eq!(docs["status"], "OPENED");
    assert_eq!(site["status"], "SKIPPED");
}

#[test]
fn open_dry_run_launches_nothing_and_exits_zero() {
    let env = TestEnv::new("dry-run");
    env.write("m.murl.json", VALID_MANIFEST);
    env.run(&["name", "add", "t", "m.murl.json"]);
    let out = env.run(&["open", "murl://local/t", "--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("dry run"));
}

#[test]
fn keygen_sign_verify_and_tamper_detection() {
    let env = TestEnv::new("signing");
    env.write("m.murl.json", VALID_MANIFEST);

    let out = env.run(&["keygen"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let out = env.run(&["sign", "m.murl.json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let out = env.run(&["verify", "m.murl.json"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(stdout(&out).contains("VALID"));

    // Tamper: flip the name.
    let path = env.root.join("work/m.murl.json");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("Test Project", "Evil Project")).unwrap();
    let out = env.run(&["verify", "m.murl.json"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(stdout(&out).contains("INVALID"));
}

#[test]
fn verify_unsigned_exits_2() {
    let env = TestEnv::new("verify-unsigned");
    env.write("m.murl.json", VALID_MANIFEST);
    let out = env.run(&["verify", "m.murl.json"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stdout(&out).contains("UNSIGNED"));
}

#[test]
fn trust_add_list_remove_roundtrip() {
    let env = TestEnv::new("trust");
    let out = env.run(&["keygen", "--out", "signer.key.json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    // Pin from the key file (extracts publicKey).
    let out = env.run(&["trust", "add", "example.com", "signer.key.json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let key_id = stdout(&out)
        .split_whitespace()
        .find(|w| w.starts_with("ed25519:"))
        .unwrap()
        .to_string();

    let out = env.run(&["trust", "list"]);
    assert!(stdout(&out).contains("example.com"));
    assert!(stdout(&out).contains(&key_id));

    let out = env.run(&["trust", "remove", "example.com", &key_id]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = env.run(&["trust", "list"]);
    assert!(!stdout(&out).contains("example.com"));
}

#[test]
fn signed_and_pinned_manifest_resolves_as_trusted_remotely() {
    let env = TestEnv::new("remote-trusted");
    env.write("m.murl.json", VALID_MANIFEST);
    env.run(&["keygen"]);
    env.run(&["sign", "m.murl.json"]);
    let signed = std::fs::read_to_string(env.root.join("work/m.murl.json")).unwrap();

    let (port, _server) = serve_manifest("/.well-known/murl/demo.murl.json", signed);
    let authority = format!("127.0.0.1:{port}");
    let murl = format!("murl://{authority}/demo");

    // Unpinned: signed but unknown key.
    let out = env.run(&["--json", "resolve", &murl]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v = json(&out);
    assert_eq!(v["nodes"][0]["trust"]["status"], "signedUnknownKey", "{v}");

    // Pin, refresh, resolve again: trusted.
    let out = env.run(&["trust", "add", &authority, "m.murl.json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = env.run(&["--json", "--refresh", "resolve", &murl]);
    let v = json(&out);
    assert_eq!(v["nodes"][0]["trust"]["status"], "signedTrusted", "{v}");

    // And the cache now serves it offline.
    let out = env.run(&["--json", "--offline", "resolve", &murl]);
    assert!(out.status.success(), "{}", stderr(&out));

    let out = env.run(&["cache", "list"]);
    assert!(stdout(&out).contains(&format!("murl://{authority}/demo")));
}

#[test]
fn offline_uncached_remote_fails() {
    let env = TestEnv::new("offline-fail");
    let out = env.run(&["--offline", "resolve", "murl://example.com/nope"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("offline"), "{}", stderr(&out));
}

#[test]
fn os_status_runs_everywhere() {
    let env = TestEnv::new("os-status");
    let out = env.run(&["os", "status"]);
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn handler_registration_roundtrip() {
    let env = TestEnv::new("handlers");
    let out = env.run(&[
        "handler",
        "register",
        "myapp",
        "--",
        "myapp-open",
        "{target}",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = env.run(&["handler", "list"]);
    assert!(stdout(&out).contains("custom:myapp"));
    let out = env.run(&["handler", "remove", "myapp"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = env.run(&["handler", "list"]);
    assert!(!stdout(&out).contains("custom:myapp"));
}

/// Serve one path from a loopback HTTP server on an ephemeral port. The
/// server thread lives until the listener is dropped.
fn serve_manifest(path: &'static str, body: String) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        // Serve a bounded number of requests, then exit.
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 4096];
            let mut req = Vec::new();
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        req.extend_from_slice(&buf[..n]);
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let request = String::from_utf8_lossy(&req);
            let ok = request.lines().next().is_some_and(|l| l.contains(path));
            let response = if ok {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/murl+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
            } else {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            };
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (port, handle)
}
