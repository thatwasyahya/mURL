//! Regressions for defects found by the adversarial review of the
//! v0.2–v1.0-prep work. Each test names the bug it prevents from returning;
//! all of them passed *incorrectly* before the corresponding fix.

use std::path::PathBuf;
use std::process::{Command, Output};

struct Env {
    root: PathBuf,
}

impl Env {
    fn new(tag: &str) -> Env {
        let root = std::env::temp_dir().join(format!(
            "murl-reg-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("work")).unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        Env { root }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_murl"))
            .args(args)
            .current_dir(self.root.join("work"))
            .env("MURL_CONFIG_DIR", self.root.join("config"))
            .env("MURL_DATA_DIR", self.root.join("data"))
            .env("MURL_CACHE_DIR", self.root.join("cache"))
            .env("MURL_SOCKET", self.root.join("run/murl.sock"))
            .env_remove("MURL_LOG")
            .output()
            .expect("binary runs")
    }

    fn write(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.root.join("work").join(rel);
        std::fs::write(&path, content).unwrap();
        path
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn out(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Build a bundle by hand. `entries` is (identity, manifest JSON).
fn bundle_json(root: Option<&str>, entries: &[(Option<&str>, String)]) -> String {
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|(identity, manifest)| {
            let bytes = manifest.as_bytes();
            let mut entry = serde_json::json!({
                "name": "Entry",
                "integrity": integrity(bytes),
                "bytes": b64(bytes),
            });
            if let Some(id) = identity {
                entry["identity"] = serde_json::json!(id);
            }
            entry
        })
        .collect();
    let mut bundle = serde_json::json!({ "bundleVersion": "0.2", "entries": items });
    if let Some(root) = root {
        bundle["root"] = serde_json::json!(root);
    }
    serde_json::to_string_pretty(&bundle).unwrap()
}

fn manifest(id: Option<&str>, name: &str) -> String {
    let mut doc = serde_json::json!({
        "murlVersion": "0.2",
        "name": name,
        "resources": [{"id": "a", "kind": "https", "target": "https://example.com/x"}]
    });
    if let Some(id) = id {
        doc["id"] = serde_json::json!(id);
    }
    serde_json::to_string(&doc).unwrap()
}

/// Finding 2: two entries that re-home onto the same local name silently
/// overwrote each other, and the bundle author chose the winner.
#[test]
fn import_refuses_two_entries_claiming_one_local_name() {
    let env = Env::new("collision");
    let json = bundle_json(
        None,
        &[
            (
                Some("murl://vendor.example/tools"),
                manifest(None, "Vendor Tools"),
            ),
            (
                Some("murl://attacker.example/tools"),
                manifest(None, "Attacker Tools"),
            ),
        ],
    );
    env.write("b.murlbundle.json", &json);

    let o = env.run(&["import", "b.murlbundle.json"]);
    assert_ne!(o.status.code(), Some(0), "{}", out(&o));
    assert!(
        out(&o).contains("both install as"),
        "expected a collision error, got: {}",
        out(&o)
    );
    // And nothing was written: the refusal happens before any store write.
    let o = env.run(&["name", "list"]);
    assert!(!out(&o).contains("tools"), "{}", out(&o));
}

/// Finding 3: re-homing round-tripped through decoded segment text, so an
/// escaped byte became grammar again — `tool%401` landed in the pinned
/// `tool@1` slot of an unrelated name.
#[test]
fn import_does_not_reparse_decoded_segments_as_grammar() {
    let env = Env::new("rehome");
    let json = bundle_json(
        None,
        &[(
            Some("murl://vendor.example/tool%401"),
            manifest(None, "Tool"),
        )],
    );
    env.write("b.murlbundle.json", &json);

    let o = env.run(&["import", "b.murlbundle.json"]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));

    let listed = out(&env.run(&["name", "list"]));
    // The whole decoded segment stays one segment; it must not become the
    // version-pinned slot of the name `tool`.
    assert!(listed.contains("tool%401"), "{listed}");
    assert!(!listed.contains("murl://local/tool@1"), "{listed}");
}

/// Finding 4: import skipped the identity binding that `name add` enforces,
/// writing store entries the resolver would always refuse to read.
#[test]
fn import_refuses_manifests_whose_id_cannot_resolve_locally() {
    let env = Env::new("idbind");
    let json = bundle_json(
        None,
        &[(
            Some("murl://vendor.example/tools"),
            manifest(Some("murl://vendor.example/tools"), "Vendor Tools"),
        )],
    );
    env.write("b.murlbundle.json", &json);

    let o = env.run(&["import", "b.murlbundle.json"]);
    assert_ne!(o.status.code(), Some(0), "{}", out(&o));
    assert!(out(&o).contains("cannot resolve as"), "{}", out(&o));
    // Previously this exited 0 and left an entry that every resolve refused.
    let o = env.run(&["name", "list"]);
    assert!(!out(&o).contains("tools"), "{}", out(&o));
}

/// Finding 5: a bundle exported from a bare manifest file has no
/// identities, and `--as` was unreachable — the error named a branch that
/// could not be taken.
#[test]
fn as_name_rescues_a_bundle_exported_from_a_file() {
    let env = Env::new("asname");
    env.write("m.murl.json", &manifest(None, "Bare"));

    let o = env.run(&["export", "m.murl.json", "-o", "bare.murlbundle.json"]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));

    // Without --as it still fails, with a message that fits the situation.
    let o = env.run(&["import", "bare.murlbundle.json"]);
    assert_ne!(o.status.code(), Some(0), "{}", out(&o));

    let o = env.run(&["import", "bare.murlbundle.json", "--as", "mine"]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    let listed = out(&env.run(&["name", "list"]));
    assert!(listed.contains("murl://local/mine"), "{listed}");
}

/// Finding 1 (config half): a configured policy must reach every path.
/// The CLI half is checked here; the daemon half is checked by the daemon's
/// own tests plus the shared loader's unit tests.
#[test]
fn configured_policy_is_honored() {
    let env = Env::new("policy");
    std::fs::write(
        env.root.join("config/config.json"),
        br#"{"policy":{"safe":"deny","sensitive":"deny","dangerous":"deny"}}"#,
    )
    .unwrap();
    env.write("m.murl.json", &manifest(Some("murl://local/p"), "P"));
    let o = env.run(&["name", "add", "p", "m.murl.json"]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));

    // Even with --yes, a deny policy denies: flags consent, they do not
    // override policy.
    let o = env.run(&["--json", "open", "murl://local/p", "--yes"]);
    assert_eq!(o.status.code(), Some(4), "{}", out(&o));
    assert!(out(&o).contains("DENIED"), "{}", out(&o));
}

/// Finding 1 (offline half): `--offline` could not be expressed in the
/// daemon protocol and was silently dropped — a fail-*open*. With no daemon
/// running the flag must still be honored in-process, and `--daemon`
/// together with `--offline` must refuse rather than quietly ignore one.
#[test]
fn offline_is_never_silently_dropped() {
    let env = Env::new("offline");
    // No daemon is running, so this exercises the fallback path.
    let o = env.run(&["--offline", "resolve", "murl://example.com/nope"]);
    assert_ne!(o.status.code(), Some(0));
    assert!(out(&o).contains("offline"), "{}", out(&o));

    let o = env.run(&["--daemon", "--offline", "open", "murl://example.com/nope"]);
    assert_ne!(o.status.code(), Some(0), "{}", out(&o));
    let text = out(&o);
    assert!(
        text.contains("--offline") || text.contains("daemon could not be used"),
        "{text}"
    );
}

/// Gap (a): the OS-handler entry point must ignore approval flags, because
/// on Windows the activated URL is substituted into a command template
/// before argv is parsed, so a crafted link can append arguments.
#[test]
fn open_url_ignores_injected_approval_flags() {
    let env = Env::new("openurl");
    env.write("m.murl.json", &manifest(Some("murl://local/p"), "P"));
    env.run(&["name", "add", "p", "m.murl.json"]);
    // Point dispatch at a no-op in case anything does launch.
    // A no-op opener, in case anything does reach dispatch. Unix-only: the
    // stub is a shell script, and on Windows the platform default is fine
    // because this test never gets past consent anyway.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let stub = env.root.join("noop-opener");
        std::fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o700)).unwrap();
        // Built with serde_json, never by formatting a string: a Windows
        // path carries backslashes, and those are JSON escapes.
        let handlers = serde_json::json!({ "open": [stub.to_string_lossy()] });
        std::fs::write(
            env.root.join("config/handlers.json"),
            serde_json::to_vec(&handlers).unwrap(),
        )
        .unwrap();
    }

    // Exactly what an injected argv would look like after the OS split it.
    let o = env.run(&["open-url", "murl://local/p", "--allow-dangerous", "--yes"]);
    // Extra arguments are rejected outright (clap), or ignored — either way
    // nothing may be approved without a prompt. Non-interactive: exit 4.
    assert_ne!(o.status.code(), Some(0), "{}", out(&o));

    // And the honest form of the same command still fails closed rather
    // than approving anything.
    let o = env.run(&["--json", "open-url", "murl://local/p"]);
    assert_eq!(o.status.code(), Some(4), "{}", out(&o));
    assert!(out(&o).contains("DENIED"), "{}", out(&o));
}

// ---------------------------------------------------------------- helpers

/// Standard base64, so the test needs no extra dependency.
fn b64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// `sha256-<base64>` over the bytes, matching murl_core::trust::make_integrity.
fn integrity(bytes: &[u8]) -> String {
    format!("sha256-{}", b64(&sha256(bytes)))
}

/// Minimal SHA-256 (FIPS 180-4), so the fixtures stay dependency-free.
fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = input.to_vec();
    let bitlen = (input.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (i, v) in [a, b, c, d, e, f, g, hh].iter().enumerate() {
            h[i] = h[i].wrapping_add(*v);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}
