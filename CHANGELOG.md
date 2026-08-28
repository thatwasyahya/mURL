# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[SemVer](https://semver.org) (pre-1.0: minor versions may break anything).

## [Unreleased]

## [0.4.0] — 2026-08-28

Three roadmap milestones (v0.3 daemon, v0.4 platform completeness, v1.0
preparation) plus the fixes from an adversarial review of all of it.

### Fixed — adversarial review

Eleven defects, each with a regression test that fails without its fix.
Every one lived in a seam between two components that looked correct alone:

* **The daemon ignored the user's configuration** — it built
  `Policy::default()` and platform-default handlers and never read
  `config.json`/`handlers.json`, so a configured `"dangerous": "deny"`
  became a clickable prompt and configured handlers vanished. Since `murl
  open` routes through the daemon by default, this was the normal path.
  Configuration now lives in `murl_core::config`, read by one loader.
  (threat T-18)
* **`--offline` was silently dropped** when routing through the daemon — a
  fail-*open*. Flags the protocol cannot carry now make the CLI decline the
  daemon path; `--daemon` turns that into an explicit error.
* **Argument injection through the Windows OS handler**: the activated URL
  is substituted into a command template before argv is parsed, so a link
  containing a quote could append `--allow-dangerous`. All platforms now
  register `murl open-url`, which ignores approval flags. (threat T-17)
* **Four bundle-import defects**: entries silently overwriting each other,
  decoded segments re-parsed as grammar (`tool%401` landing in the pinned
  `tool@1` slot), missing identity binding, and an unreachable `--as`.
* **Tilde paths escaping the home directory** (`Path::join` replaces the
  base on an absolute argument, so `~//etc/passwd` opened `/etc/passwd`
  while the prompt showed a home path), **empty handler templates executing
  the target as a program**, **Windows trailing-dot executable
  classification**, and an **unbounded daemon read**.

### Added — v0.4 platform completeness

* **macOS app bundle** (`packaging/macos/build-app.sh` + `Info.plist.in`):
  Launch Services reads scheme claims only from an application bundle, so
  registration needs one. The bundle wraps the same binary; the launcher
  stub passes the activated URL through untouched.
* **Four new resource kinds**, each with a security review and conformance
  vectors: `ssh` and `remote-desktop` (DANGEROUS, handler-gated, option
  smuggling refused), `geo` and `mailto` (SAFE, range-checked and
  header-allow-listed respectively).
* `murl handler set-ssh` / `set-remote-desktop`.

### Added — v0.3 the daemon

* **`murl-daemon`**: a resident resolver providing a persistent consent
  surface, over a user-private socket (0600 in a 0700 directory, ownership
  verified by both sides). Its IPC threat model (D-1 … D-7) is documented
  in [docs/daemon.md](docs/daemon.md) and was written before the code.
* **`ConsentUi` abstraction** so a GUI dialog can replace the terminal
  prompt without touching the protocol or the security model. Policy
  denials survive a deliberately rogue surface (tested).
* **`murl-net`**: the hardened HTTPS fetcher extracted so the CLI and the
  daemon share one implementation.
* CLI `--daemon` / `--no-daemon`; the default tries the daemon and falls
  back silently. The daemon is never a dependency.

### Added — v1.0 preparation

* [docs/stability.md](docs/stability.md): stability labels, post-1.0
  compatibility rules, deprecation policy, and the security exception.
  `#[non_exhaustive]` applied to the enums expected to grow.
* [spec/registration/](spec/registration/): IANA templates for the `murl`
  URI scheme (RFC 7595 provisional) and `application/murl+json`
  (RFC 6838/6839), drafted but deliberately not submitted — registration
  is gated on a second implementation.
* `spec/check-schema.py` in CI: the descriptive schema must accept every
  valid vector and is held honest about which invalid ones it cannot catch.
* `examples/daemon-demo.sh`, also run in CI.

## [0.2.0] — 2026-08-27

Format hardening. The manifest format is now strict enough to freeze, and
independent implementations have something to test against.

### Added

* **Conformance suite** (`spec/conformance/`): 14 valid + 34 invalid
  manifests and 79 mURL vectors, with a documented contract and a Rust
  harness (`crates/murl-core/tests/conformance.rs`) other implementations
  can copy.
* **JSON Schema** for the manifest (`spec/murl-manifest.schema.json`),
  explicitly descriptive — the reference validator stays normative.
* **`notBefore`** manifest member: a validity floor to pair with `expires`.
  Outside either bound, only SAFE resources reach a prompt.
* **Selector extensions**: multiple comma-separated items with union
  semantics, plus `role=` and `tag=` forms (`#docs,role=monitoring`).
  Every item must match something or the resolution fails.
* **`murl export` / `murl import`**: portable bundles carrying a
  destination and every manifest it composes as verbatim bytes with
  integrity hashes. Imports verify, then install into the *local*
  namespace — a bundle can never claim another authority.
* **`murl create --interactive`**: build a manifest by answering prompts,
  with per-answer validation and risk tiers shown as resources are added.

### Changed

* **Duplicate JSON members are now invalid** at every nesting level and
  rejected at parse time (spec §5.1). This closes threat T-15 at the format
  level rather than relying on a shared parser.
* Numbers must be integers *everywhere*, including inside free-form `meta`;
  non-integers are validation errors (they would be unsignable under MCF-1).
* `murlVersion` is now `"0.2"` for writers; `"0.1"` remains accepted.
* Identifier grammars (id/role/tag) are shared between the selector parser
  and the manifest validator, so the two cannot drift.

## [0.1.0] — 2026-08-27

Initial public release: the mURL primitive, end to end.

### Added

* **Specification** (`spec/SPECIFICATION.md`): `murl://` grammar with
  security-motivated deviations from generic URI syntax (userinfo
  forbidden, ASCII-only, dot segments rejected); manifest format
  (`application/murl+json`); well-known resolution; MCF-1 canonical form;
  signature block; normative limits; failure semantics.
* **murl-core**: hardened parser; manifest model + collecting validator;
  MCF-1 canonicalizer; resolver with local store, HTTPS well-known,
  integrity-checked cache, offline fallback, recursive splicing under
  depth/count/cycle limits, identity binding; SAFE/SENSITIVE/DANGEROUS
  policy engine with trust-gated DANGEROUS dispatch; ed25519
  signing/verification with per-authority key pinning; argv-only dispatch
  planning behind `Launcher`/`RemoteFetcher` traits.
* **murl CLI**: `parse`, `create`, `validate`, `inspect`, `resolve`,
  `open` (consent flow, `--dry-run`, tier-scoped allow flags, `--only`/
  `--skip`), `name`, `keygen`/`sign`/`verify`, `trust`, `cache`, `handler`,
  `os` (Linux XDG + Windows HKCU registration; macOS documented stub);
  stable exit codes (0/1/2/3/4); `--json` machine output; SSRF-guarded
  HTTPS fetcher (zero redirects, size-capped reads, private-range DNS
  filtering).
* **Tests**: 120+ unit/integration tests including dedicated resolver
  security and dispatch suites and hermetic end-to-end CLI tests with a
  loopback HTTP server.
* **Fuzzing**: cargo-fuzz targets for the parser (round-trip property),
  manifest pipeline, and canonicalizer (fixpoint property), with seed
  corpora.
* **Docs**: concept, architecture, security model, 16-entry threat model,
  trust model, resolution, resource types, OS integration, prior art,
  examples, FAQ, roadmap.
* **Examples**: Project X destination with a nested team destination and
  a hermetic `demo.sh`.
* **Project infrastructure**: CI (fmt, clippy, tests on Linux/Windows/
  macOS, cargo-deny, fuzz build+smoke), release workflow, issue/PR
  templates, dual MIT/Apache-2.0 licensing.
