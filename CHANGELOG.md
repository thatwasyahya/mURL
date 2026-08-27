# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[SemVer](https://semver.org) (pre-1.0: minor versions may break anything).

## [Unreleased]

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
