# Contributing to mURL

Thanks for considering it. mURL is an experimental protocol with a reference
implementation; contributions that sharpen either are welcome — as are
well-argued issues saying "this design is wrong because…".

## Ground rules

* **Security first.** This codebase's job is to make a powerful primitive
  safe. Changes to the parser, validator, resolver, policy, trust, or
  dispatch paths need tests demonstrating the security property they keep
  (or newly add). A feature that weakens fail-closed behavior needs an
  extraordinary justification.
* **Spec and code move together.** Behavior changes that touch the format
  or resolution semantics must update `spec/SPECIFICATION.md` (and the
  relevant `docs/`) in the same PR. The spec is normative; drift is a bug.
* **No new dependencies without a case.** The dependency set is small and
  boring on purpose; `deny.toml` is enforced in CI. Argue need, popularity,
  maintenance, and audit surface in the PR description.
* **No shell, no unsafe.** `unsafe_code` is forbidden workspace-wide, and
  no code path may pass targets through a shell. These are architectural
  invariants, not preferences.

## Workflow

1. Open an issue first for anything non-trivial (especially format
   changes) — design discussion belongs before code.
2. Fork, branch from `main`, keep PRs focused.
3. Before pushing:

```bash
cargo fmt
cargo clippy --all-targets   # must be warning-free
cargo test                   # all suites
cd fuzz && cargo check       # fuzz targets must still build
```

4. Add tests: unit tests beside the code; resolver/dispatch behavior in
   `crates/murl-core/tests/`; CLI behavior in
   `crates/murl-cli/tests/cli_integration.rs` (hermetic — use the
   `MURL_*_DIR` env pattern, never the real home directory).
5. If you touched hostile-input handling, run the relevant fuzz target for
   at least a few minutes: `cargo +nightly fuzz run parse_murl`.

## Commit and PR conventions

* Imperative subject lines ("reject duplicate keys", not "rejected…").
* Explain *why* in the body; the diff already says what.
* PRs are squash-merged; the PR description becomes the commit body — write
  it like documentation, because it will be.

## Good first contributions

Look for issues labeled `good-first-issue`, or: additional hostile-manifest
test vectors, conformance corpus entries (v0.2 goal), documentation fixes,
and platform testing of `murl os` on desktop environments we haven't tried.

## Conduct

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
Security issues go to [SECURITY.md](SECURITY.md), not the issue tracker.
