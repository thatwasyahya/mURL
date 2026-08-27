# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[SemVer](https://semver.org) (pre-1.0: minor versions may break anything).

## [Unreleased]

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
