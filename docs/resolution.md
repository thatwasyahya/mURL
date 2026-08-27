# Resolution

How `murl://…` becomes an approved, dispatched plan. Normative text lives in
spec §6; this document is the narrative walkthrough of the implementation
(`crates/murl-core/src/resolver.rs`).

## Sources, in order

For `murl://local/<name>`: the **local name store** only —
`<data>/names/<segments>/<last>[@ver].murl.json`, managed by `murl name`.
Never the network.

For `murl://<host>/<name>`:

```text
            ┌─ cache fresh? ──────────── yes ─▶ use silently (normal path)
            │
resolve ────┤                     ┌─ ok ─▶ cache + use
            └─ fetch well-known ──┤
                                  └─ fail ─┬─ stale cache? ─▶ use + WARN
                                           └─ else ─────────▶ error
   --offline: skip fetch entirely; any cache (fresh silently, stale + WARN)
```

The well-known mapping (RFC 8615 style):

```text
murl://acme.example/team/project-x        → https://acme.example/.well-known/murl/team/project-x.murl.json
murl://acme.example/team/project-x@1.4.2  → https://acme.example/.well-known/murl/team/project-x@1.4.2.murl.json
murl://localhost:8080/dev                 → http://localhost:8080/.well-known/murl/dev.murl.json
```

Publishing a namespace is therefore: put signed JSON files on a static web
server. No registry, no API, no accounts. This is the entire server-side
footprint of the protocol, and keeping it that small is a design goal.

Freshness: `@latest` entries expire after `cacheTtlSecs` (3600 s default);
pinned versions never expire (immutability is the authority's contract).
`--refresh` evicts before resolving; `murl cache list|evict|clear` manages
the store; every cached read re-verifies a sha256 of the bytes.

## Ingesting one manifest

Each manifest — root or nested — passes the same gauntlet, in order:
size cap → JSON parse → full validation → signature verification (invalid =
hard stop) → trust status determination → identity binding → expiry check.
Only then do its resources enter the plan. There is no "trusted enough to
skip validation" path.

## Splicing nested destinations

A `murl`-kind resource pulls another destination's resources into the plan
at its position:

```text
project-x
├─ source        https              ┐ own resources
├─ workspace     dir                ┘
└─ team          murl://local/demo/team
                   ├─ wiki   https  ┐ spliced children,
                   └─ chat   https  ┘ root_anchor = "team"
```

Bookkeeping per splice: depth check (≤3), manifest count (≤8), identity
pushed on the path stack (cycle ⇒ hard error), visited-set check (diamond ⇒
skip + warn), optional integrity pin over the child's raw bytes,
cross-authority warning, and every child resource counted against the
64-resource ceiling and the (kind, target) dedup set.

`root_anchor` records which *root* resource each planned entry descends
from — that's what `#selector` filters on, so `murl open murl://…/p#team`
means "just the team part", spliced children included.

## Ordering

Within each manifest: Kahn's algorithm over `dependsOn` with (order, index)
tie-breaking — deterministic plans, same input same order, which matters for
auditability as much as aesthetics. Nested resources take the position of
their container. Cycles in `dependsOn` were already validation errors;
resolution re-checks anyway (defense in depth).

## What resolution produces

A `Resolution`: the manifest nodes (origin, trust, depth, expiry), the
flattened `PlannedResource` list (kind, tier, anchor, policy decision after
`apply_policy`), and accumulated warnings. `murl resolve` prints it (or
`--json` for tooling); `murl open` carries it into consent and dispatch.
Nothing in resolution has side effects beyond cache writes — the plan is
inspectable *because* resolving is not opening.
