# Stability and Compatibility Policy

What this project promises, what it doesn't, and how you can tell which is
which. Everything below applies from **v0.5 onward**; v0.1–v0.4 were
explicitly experimental and broke things freely.

## Stability labels

Every part of the project carries one of three labels, used consistently in
the docs and in rustdoc:

| Label | Meaning |
|---|---|
| **experimental** | May change or vanish in any release, including patch releases. |
| **stable** | Changes only per the compatibility rules below. |
| **implementation-specific** | Behavior of *this* codebase, not the format. Another implementation may differ and still be conformant. |

## Current labels

| Surface | Label (v0.4) | Notes |
|---|---|---|
| `murl://` grammar (spec §3) | experimental → **stable at 1.0** | Frozen once a second implementation passes the conformance suite. |
| Manifest format (spec §5) | experimental → **stable at 1.0** | 0.1 → 0.2 was additive; further changes before 1.0 may not be. |
| MCF-1 canonical form + signature block (spec §7) | experimental → **stable at 1.0** | Changing this invalidates existing signatures, so it freezes with the format. |
| Well-known resolution path (spec §6.2) | experimental → **stable at 1.0** | |
| Conformance suite (`spec/conformance/`) | **versioned with the spec** | Vectors are added freely; existing vectors change only when the spec does. |
| `murl` CLI: command names, flags, `--json` shape | experimental → **stable at 1.0** | Exit codes (0/1/2/3/4) are already treated as a contract. |
| `murl-core` Rust API | experimental | Follows SemVer *within* pre-1.0 rules: minor versions may break. |
| `murl-daemon` wire protocol | experimental | Version-matched exactly; no negotiation. Bumping `PROTOCOL_VERSION` is the only compatibility mechanism. |
| `murl-net`, `murl-daemon` Rust APIs | implementation-specific | Not intended as public API surface; use `murl-core`. |
| Config, trust store, cache, handler file layouts | implementation-specific | Migrated automatically where practical; never part of the format. |

## After 1.0

**The format** (grammar, manifest, canonical form, signatures, resolution)
follows these rules:

* **Patch** (1.0.x): editorial fixes only. No behavior change.
* **Minor** (1.x.0): additive only — new optional manifest members, new
  kinds, new selector forms. A 1.0 consumer must ignore what it doesn't
  know (unknown members are already ignore-with-warning), and a 1.x
  producer must not require a consumer to understand additions to remain
  safe. Anything that would make an old consumer *unsafe* rather than
  merely less capable is a major change, not a minor one.
* **Major** (2.0.0): anything else, including tightening a rule that
  previously accepted something.

`murlVersion` reflects the format's major.minor. Consumers must reject a
different major and should accept same-major/newer-minor with warnings.

**The Rust crates** follow standard Cargo SemVer, with `murl-core` as the
only crate intended for external use. `#[non_exhaustive]` is applied to the
enums most likely to grow (kinds, error variants, outcome statuses) so
adding variants stays a minor change.

**The daemon protocol** is versioned by an integer with exact matching.
There is no negotiation, and there will not be: a client and daemon that
disagree fall back to in-process resolution, which is always available and
always fail-closed. A protocol change bumps the integer; old clients then
simply stop using the daemon rather than mis-parsing it.

## Deprecation

Anything stable that is going away gets: a release note, a runtime warning
where the code path allows one, and at least one full minor release of
overlap. Removals happen only in major versions. Nothing is removed
silently.

## Security exception

A change required to fix a vulnerability may break compatibility in any
release, including a patch. It will be documented as such in the changelog
and in the advisory. Security wins over compatibility, every time — the
alternative is a format whose defects are permanent.
