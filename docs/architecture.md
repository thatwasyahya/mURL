# Architecture

## Shape of the system

Two crates plus a fuzz harness. The split is a security boundary, not a
convenience:

```text
┌────────────────────────────────────────────────────────────────────┐
│ crates/murl-core        deterministic, no network, no processes    │
│                                                                    │
│  murl.rs        parser (untrusted input, fuzzed)                   │
│  manifest.rs    schema + validator (untrusted input, fuzzed)       │
│  canonical.rs   MCF-1 canonical JSON (signature substrate, fuzzed) │
│  resolver.rs    name → manifest → spliced plan                     │
│  graph.rs       dependsOn ordering, cycle detection                │
│  policy.rs      SAFE/SENSITIVE/DANGEROUS + consent decisions       │
│  trust.rs       ed25519 sign/verify, key pinning, integrity        │
│  cache.rs       integrity-checked manifest cache                   │
│  fetch.rs       LocalStore + RemoteFetcher *trait*                 │
│  dispatch.rs    argv construction + sequencing, Launcher *trait*   │
│  limits.rs      the normative resource bounds                      │
│  time.rs        Clock trait + strict RFC 3339 subset               │
└───────────────▲────────────────────────────────▲───────────────────┘
                │ RemoteFetcher                  │ Launcher
┌───────────────┴────────────────────────────────┴───────────────────┐
│ crates/murl-cli         effects live here                          │
│                                                                    │
│  httpfetch.rs   ureq + rustls, 0 redirects, size cap while         │
│                 reading, private-IP filtering at DNS time          │
│  launcher.rs    Command::new(argv[0]).args(..) — never a shell     │
│  consent.rs     plan display + TTY consent, fail-closed            │
│  ctx.rs         config, XDG paths, resolver wiring                 │
│  commands/*     parse create validate inspect resolve open name    │
│                 keygen sign verify trust cache os handler          │
└────────────────────────────────────────────────────────────────────┘
```

Everything that must be *correct under hostile input* lives in `murl-core`
behind pure interfaces, which is what makes the 100+ hermetic tests and the
three fuzz targets possible. Everything that *touches the world* lives in
the CLI behind two narrow traits (`RemoteFetcher`, `Launcher`), each with a
recording test double.

## The resolution pipeline

```text
murl://acme.example/project-x#monitoring
  │ parse                 reject: >1024B, userinfo, dot segments, bad %, …
  ▼
locate manifest           local store │ fresh cache │ HTTPS well-known
  │                       │ stale cache as explicit offline fallback
  ▼
size cap → JSON → validate           every §5 rule; errors collected, fatal
  ▼
verify signature          invalid signature = hard stop (tamper evidence)
  ▼
bind identity             manifest.id must match the requested name
  ▼
splice nested mURLs       depth ≤3, manifests ≤8, resources ≤64,
  │                       cycle detection on the identity stack,
  │                       integrity pins, (kind,target) dedup
  ▼
classify + policy         tier per resource; Deny/Prompt/Allow per resource
  ▼
Resolution                nodes + flattened PlannedResources + warnings
  ▼
consent (CLI)             plan shown; flags or TTY answers; fail closed
  ▼
execute                   ordered, staggered, argv-only; per-resource
                          outcome → aggregate status
```

## Decisions and their reasons

**Rust.** Memory-safe parsing of hostile input, `#![forbid(unsafe_code)]`
across the workspace, first-class fuzzing (cargo-fuzz/libFuzzer), static
binaries for an OS-registered handler, and a type system that lets the
plan/consent/dispatch state machine be encoded rather than documented. Go
was the serious alternative (faster builds, simpler onboarding); Rust won on
parser safety and fuzzing ergonomics for a project whose core artifact *is*
a parser for untrusted input.

**Dependencies are few and boring.** serde/serde_json, thiserror,
ed25519-dalek + sha2 + base64 + getrandom, clap, ureq(rustls). No async
runtime — resolution is a handful of sequential fetches; the complexity
budget is spent on security, not on an executor. `cargo-deny` enforces the
dependency policy in CI.

**No daemon in v0.1 — deliberately.** The OS handler invokes the CLI per
activation. A daemon (`murl-daemon`) earns its place only when something
needs persistence: a native consent dialog, cross-app IPC, background cache
refresh. Until then it would be an always-on attack surface plus an IPC
protocol to secure, purchased before the format has even stabilized. The
CLI/core split means the daemon, when it comes (roadmap v0.3), reuses core
unchanged.

**Manifest keeps raw + typed forms.** Signatures cover the MCF-1 canonical
form of the *raw* document, so unknown members from future spec versions
stay signed. Re-serializing the typed struct would silently strip them —
a forward-compatibility hazard with a security flavor.

**Graph, deliberately small.** The prompt asks whether the resource set
should be a full graph. v0.1 answers: the *data model* admits a graph
(`relations` edges are typed and validated), but only `dependsOn` has
runtime semantics (dispatch ordering, acyclicity enforced). Rich graph
semantics (queries, transitive activation, relation-driven policy) are
deferred until a real consumer exists — speculative graph machinery in a
security-sensitive resolver is cost without a customer.

**Sequential, staggered dispatch.** Launching N applications concurrently
is worse UX (window-manager chaos) *and* a resource-exhaustion vector. The
plan launches in dependsOn/order sequence with a 150 ms stagger.

**Errors are staged.** `Error` variants map 1:1 onto pipeline stages
(Parse/Manifest/Validation/Resolution/Limit/Cycle/Trust/Denied/Fetch/
Dispatch), which gives the CLI stable exit codes (0/1/2/3/4) without string
matching.

## Extension points

* **New resource kinds**: `custom:<name>` + `murl handler register` today;
  new built-in kinds are a spec change (kind.rs + policy.rs + dispatch.rs).
* **New manifest sources**: implement `RemoteFetcher` (e.g. IPFS, an
  internal registry) — the resolver does not care.
* **New platforms**: `OpenerConfig::platform_default` + `os_cmd.rs`
  registration; dispatch itself is already platform-neutral argv.
* **Embedders**: a GUI or daemon links `murl-core`, implements the two
  traits, and inherits the whole pipeline including policy and trust.
