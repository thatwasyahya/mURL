# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[SemVer](https://semver.org) (pre-1.0: minor versions may break anything).

## [Unreleased]

### Added

* **Browser playground** at <https://thatwasyahya.github.io/mURL/playground.html>:
  paste the resources that make up one piece of work and get a validated
  manifest back, with every kind inferred, every resource classified into its
  tier, and every rule checked. The checking is done by `murl-core` itself,
  compiled to WebAssembly (`crates/murl-wasm`), not by a second copy of the
  rules written in JavaScript — the kinds, tiers and validation messages on
  that page are the ones the CLI produces, by construction. Nothing typed
  into it leaves the browser.
* `murl-core` gained a default-on `keygen` feature. Generating a signing key
  is the only thing in the crate that needs entropy; turning the feature off
  drops `Keypair::generate` and lets the crate build for
  `wasm32-unknown-unknown` with no JavaScript shim. Loading keys, signing and
  verifying are unaffected.

### Changed

* The documentation site was redesigned: a real design system (self-hosted
  IBM Plex and Instrument Serif, one accent, tier colours reserved for
  tiers), dark and light themes, syntax-highlighted code, a table of contents
  on the specification and security pages, and an animated resolution
  diagram on the front page. Every library it uses is vendored and pinned
  (`docs/site/vendor/VENDOR.md`); the site makes no request to a third-party
  host at runtime.

## [0.5.0] — 2026-08-29

The release where consent got a real surface, the format got a second
implementation, and CI ran for the first time.

### Added

* **Native consent dialog** (`murl-daemon`), built on the helper each desktop
  already ships — `zenity`, `kdialog`, or `osascript` — rather than a toolkit
  dependency. The daemon picks the best surface available and says which one:
  dialog, then terminal, then denial, a chain that only ever gets stricter.
  This is also what makes macOS usable: a Launch Services activation has no
  controlling terminal, so consent there could previously only refuse.
  * The AppleScript source is a **constant** and the plan travels in `argv`;
    interpolating a target into script text would be the same mistake as
    building a shell command, one language over.
  * The dialog returns **resource ids and nothing else**, and anything
    returned that was not offered is discarded — a backend cannot grant what
    policy denied. A test drives a rogue backend to prove it.
  * Every failure is a denial: no backend, crash, cancel, closed window,
    unparseable output, or no answer within 180 s.
* **`murl-daemon service install|uninstall|status`** — a systemd *user* unit
  or a LaunchAgent, never a system unit or a LaunchDaemon. The daemon holds
  no capability the user lacks; installing it system-wide would grant it one.
* **A second implementation**: `reference/python/`, the v0.2 format in
  pure-stdlib Python, written from the specification. It passes every
  conformance vector.
* **Canonical-form conformance vectors** (`spec/conformance/canonical/`) and
  a fifth conformance rule. See "Fixed" — their absence was a real hole.
* **Distribution**: Homebrew, Scoop, AUR, Nix and winget manifests, plus
  `docs/install.md`; a static documentation site (`docs/site/`) with a Pages
  workflow; `RELEASING.md`; and `.github/seed-issues.sh`.
* **Benchmarks** (`cargo run --release -p murl-core --example bench`): a
  maximum-legal 64-resource manifest validates in ~82 µs and an ed25519
  verification costs ~56 µs, against a network fetch measured in
  milliseconds. The limits in spec §6.6 bound hostile input; they are not
  compensating for slow parsing.

### Fixed

* **The conformance suite never tested the canonical form.** It covered the
  grammar and the schema and skipped the one artifact that must agree
  byte-for-byte for signatures to interoperate. A second implementation
  could pass all 137 vectors with a silently wrong canonical form — this one
  nearly did. Both implementations are now verified byte-identical.
* **Specification §7.1** left three things unsaid, each of which produces
  signatures nobody else can verify: the hex case in `\u00xx` escapes (it is
  lowercase), whether U+007F is escaped (it is not), and the number rules
  beyond "integers". The claim that MCF-1 is byte-identical to RFC 8785 was
  **overstated** and is corrected: JCS sorts by UTF-16 code unit, MCF-1 by
  code point, and they disagree on documents the schema permits.
* Three more spec gaps: the grammar admitted `local:80` while the parser
  rejected it, `qchar` was used and never defined, and a vector in
  `manifests/valid/` contradicted the §7.2 `keyId` MUST.
* `AggregateStatus::Partial` serialized as `PARTIAL` while the specification
  and `Display` both said `PARTIAL_SUCCESS`, so a consumer matching the
  documented string reported failure for a partially successful activation.
* **Platform defects CI caught the moment it could run**: macOS tests stubbed
  the opener with `/bin/true`, which lives in `/usr/bin` there; Windows could
  not execute the `#!/bin/sh` dialog stubs, so the id-mapping rules went
  untested on the platform whose OS handler is most exposed (they are now a
  pure function tested everywhere); and a test hand-formatted a Windows path
  into JSON, where backslashes are escapes.

### Changed

* **MSRV is now 1.88**, measured rather than claimed: 1.88 builds the
  workspace and 1.85 does not. The floor comes entirely from
  `ureq → url → idna → icu_*`. `murl-core` — the crate an embedder or a
  second implementation depends on — pulls none of that chain.
  `RELEASING.md` records the two ways to lower it and why neither was taken.

### Note

CI had never executed a single step before this release. Nine jobs were
created and killed in seconds each, with no logs, because a private
repository has no Actions minutes on this account — which reads exactly like
a broken workflow and is not one. Making the repository public fixed it, and
the first real run found five genuine defects, all fixed above.


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
