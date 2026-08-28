//! A dependency-free benchmark for the hot paths.
//!
//! Run with:  cargo run --release -p murl-core --example bench
//!
//! Why an example rather than a `#[bench]` or criterion: `#[bench]` needs
//! nightly, and criterion is a large dependency tree for a project whose
//! dependency policy is the point. This measures the three things whose
//! cost is actually load-bearing — parsing an identifier, validating a
//! manifest, and canonicalizing one for signing — plus signature
//! verification, which is the only operation with a *fixed* large cost.
//!
//! What the numbers are for: the resolution limits (spec §6.6) exist to
//! bound what a hostile input can cost. If validating a maximum-size
//! manifest were expensive, the 256 KiB cap would be the wrong number.

use std::hint::black_box;
use std::time::Instant;

use murl_core::canonical::canonical_json_bytes;
use murl_core::manifest::Manifest;
use murl_core::murl::Murl;
use murl_core::trust::{sign_manifest, verify_manifest, Keypair};
use murl_core::Limits;

fn bench<F: FnMut()>(name: &str, iterations: u32, mut f: F) {
    // A warm-up pass so the first measurement is not paying for page faults
    // and branch predictors.
    for _ in 0..iterations.min(1_000) {
        f();
    }
    let started = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = started.elapsed();
    let per = elapsed.as_secs_f64() / f64::from(iterations);
    let (value, unit) = if per < 1e-6 {
        (per * 1e9, "ns")
    } else if per < 1e-3 {
        (per * 1e6, "µs")
    } else {
        (per * 1e3, "ms")
    };
    println!("{name:<44} {value:>9.2} {unit}   {:>12.0} ops/s", 1.0 / per);
}

/// A manifest with `n` resources, as an author would plausibly write it.
fn manifest_json(n: usize) -> String {
    let mut resources = Vec::with_capacity(n);
    for i in 0..n {
        resources.push(format!(
            r#"{{"id":"res{i}","kind":"https","target":"https://example.com/service/{i}","label":"Resource {i}","role":"docs","order":{},"tags":["team","generated"]}}"#,
            (i % 100) * 10
        ));
    }
    format!(
        r#"{{"murlVersion":"0.2","id":"murl://local/bench","name":"Benchmark","description":"A synthetic destination.","resources":[{}]}}"#,
        resources.join(",")
    )
}

fn main() {
    let limits = Limits::default();

    println!("mURL hot paths (release build)\n");

    // --- identifier parsing: every activation starts here, and it is the
    // one function that runs on wholly untrusted input before anything else.
    bench("parse murl://local/project-x", 200_000, || {
        black_box(Murl::parse(black_box("murl://local/project-x")).unwrap());
    });
    bench("parse deep name + version + selector", 200_000, || {
        black_box(
            Murl::parse(black_box(
                "murl://example.com/a/b/c/d/e/f/g@1.4.2#docs,role=monitoring,tag=ops",
            ))
            .unwrap(),
        );
    });
    bench("reject hostile identifier (userinfo)", 200_000, || {
        black_box(Murl::parse(black_box("murl://github.com@evil.example/x")).unwrap_err());
    });

    // --- manifests: small is the common case, 64 resources is the ceiling
    // the specification allows.
    for n in [2usize, 16, 64] {
        let json = manifest_json(n);
        let bytes = json.as_bytes();
        bench(&format!("parse manifest ({n} resources)"), 20_000, || {
            black_box(Manifest::from_slice(black_box(bytes), &limits).unwrap());
        });
        let manifest = Manifest::from_slice(bytes, &limits).unwrap();
        bench(
            &format!("validate manifest ({n} resources)"),
            20_000,
            || {
                black_box(manifest.validate());
            },
        );
        bench(
            &format!("canonicalize (MCF-1, {n} resources)"),
            20_000,
            || {
                black_box(canonical_json_bytes(black_box(&manifest.raw)).unwrap());
            },
        );
    }

    // --- the size cap, measured: a maximum-legal manifest is what the
    // 256 KiB limit is really protecting against.
    let big = manifest_json(64);
    println!(
        "\nmaximum-size manifest in this benchmark: {} bytes (cap is {})",
        big.len(),
        limits.max_manifest_bytes
    );

    // --- signatures: fixed cost, and the only place we spend real CPU.
    let keypair = Keypair::generate().unwrap();
    let mut signed = Manifest::from_slice(manifest_json(16).as_bytes(), &limits)
        .unwrap()
        .raw;
    sign_manifest(&mut signed, &keypair).unwrap();
    bench("sign manifest (ed25519, 16 resources)", 2_000, || {
        let mut copy = signed.clone();
        sign_manifest(&mut copy, &keypair).unwrap();
        black_box(&copy);
    });
    bench("verify signature (ed25519)", 2_000, || {
        black_box(verify_manifest(black_box(&signed)).unwrap());
    });

    println!(
        "\nReading these: resolution is dominated by I/O — a network fetch is\n\
         milliseconds, and dispatch waits on process spawn plus a deliberate\n\
         150 ms stagger. Everything measured above is far below that, which is\n\
         the intended shape: the limits in spec §6.6 exist to bound hostile\n\
         input, not to compensate for slow parsing."
    );
}
