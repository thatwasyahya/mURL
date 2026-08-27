# mURL Specification

**Multi-Resource Uniform Locator — version 0.1**

Status: **experimental draft**. This document is the normative reference for
the mURL v0.1 format and resolution behavior, as implemented by the reference
implementation in this repository. Everything in it may change before 1.0.
Nothing in it is an internet standard, and it must not be presented as one.

The key words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY are to be
interpreted as described in RFC 2119.

---

## 1. Purpose

A URL identifies one resource. An mURL identifies a **logical destination**:
a named, versionable, verifiable *set* of heterogeneous resources.

```text
URL   :  identifier ──────────────▶ resource
mURL  :  identifier ──▶ manifest ──▶ { resource, resource, resource, … }
```

The indirection through a **manifest** is a load-bearing design decision, not
an implementation detail:

* the identifier stays short, stable, and shareable while the set changes;
* the set has a canonical byte form, so it can be hashed and **signed**;
* the set can be cached, versioned, and diffed;
* the set can be inspected *before* anything is opened — which is what makes
  a meaningful consent step possible at all.

An mURL therefore MUST NOT embed resources directly. It is a name, never a
container.

## 2. Terminology

| Term | Meaning |
|---|---|
| **mURL** | An identifier of the form `murl://authority/name[@version][?query][#selector]`. |
| **Authority** | The namespace owner: the reserved word `local`, or a DNS name. |
| **Manifest** | The JSON document a name resolves to (`application/murl+json`). |
| **Resource** | One entry in a manifest: a kind plus a kind-specific target. |
| **Kind** | The type of a resource (`https`, `file`, `dir`, `murl`, `terminal`, `custom:*`). |
| **Resolver** | Software that maps an mURL to a manifest and a dispatch plan. |
| **Activation** | The user-approved dispatch of a resolved plan. |
| **Tier** | The risk classification of a resource: SAFE, SENSITIVE, DANGEROUS. |

## 3. mURL syntax

### 3.1 Grammar

```abnf
murl        = "murl://" authority "/" name [ "@" version ]
              [ "?" query ] [ "#" selector ]

authority   = "local" / host [ ":" port ]
host        = label *( "." label )            ; lowercase after parsing
label       = 1*63( lc-alnum / "-" )          ; no leading/trailing "-"
port        = 1*5DIGIT                        ; 1..65535

name        = segment *( "/" segment )        ; 1..8 segments
segment     = 1*( seg-char / pct-encoded )    ; ≤64 bytes decoded
seg-char    = %x21-7E except "/" "?" "#" "@" "%"
pct-encoded = "%" HEXDIG HEXDIG

version     = "latest" / vnum *2( "." vnum )
vnum        = 1*5DIGIT                        ; no leading zeros

selector    = 1*64( lc-alnum / "-" / "_" )
query       = *512qchar                       ; reserved, no v0.1 semantics
lc-alnum    = %x61-7A / DIGIT
```

### 3.2 Constraints (all MUST)

| Constraint | Value | Rationale |
|---|---|---|
| Total length | ≤ 1024 bytes | resource-exhaustion bound |
| Character set | printable ASCII (0x21–0x7E) | non-ASCII must be percent-encoded UTF-8; removes homoglyph attacks from the raw identifier |
| Userinfo | **forbidden** | `murl://github.com@evil.example/x` must be a parse error, not a phishing vector |
| IPv6 literals | not supported in v0.1 | keeps the authority grammar auditable; IPv4 dotted-quads parse as reg-names |
| Name segments | 1–8 segments, ≤64 bytes each after decoding | bounds store paths and URLs |
| Dot segments | `.` and `..` forbidden, including percent-encoded forms | path traversal |
| Decoded segments | valid UTF-8, no control characters, no `/` or `\` | injection into stores/URLs |
| `@` | at most once, only as the version marker on the final segment | unambiguous parse |
| Authority case | folded to lowercase | DNS semantics |
| IDN authorities | must be punycoded (`xn--…`) | parser rejects non-ASCII authorities |

A parser MUST reject any input violating these rules. A parser MUST NOT
attempt to repair malformed input: a parser that guesses is a parser that can
be steered.

### 3.3 Canonical form and identity

The **canonical form** of an mURL lowercases the scheme and authority,
percent-encodes each decoded segment minimally (unreserved characters
`A-Z a-z 0-9 - . _ ~` literal, everything else `%XX` uppercase hex), elides
`@latest`, and preserves query and selector.

The **identity** of an mURL is its canonical form with query and selector
stripped. Identity is the unit of:

* cache keying,
* cycle detection during recursive resolution,
* the `id` binding check (§6.4).

### 3.4 Examples

Valid:

```text
murl://local/project-x
murl://local/team/project-x@1.4.2
murl://example.com/platform/checkout#monitoring
murl://127.0.0.1:8443/dev
murl://local/caf%C3%A9
```

Invalid (and why):

```text
murl://github.com@evil.example/x     userinfo forbidden
murl://local/../etc                  dot segment
murl://local/a%2Fb                   encoded path separator
murl://local/x@01                    leading zero in version
murl://[::1]/x                       IPv6 literal
murl:local/x                         missing //
```

## 4. Authorities and namespaces

mURL introduces **no new registry**. Namespace ownership is:

* **`local`** (reserved): names resolve against the user's local name store.
  Resolution never touches the network. The user owns this namespace
  entirely.
* **DNS names**: whoever controls `example.com` (and can serve HTTPS for it)
  controls the `murl://example.com/…` namespace. This inherits DNS's
  properties wholesale: global uniqueness, existing operational practice,
  existing revocation (stop serving), and existing weaknesses (domains
  change hands — mitigated by signing, §7).

Alternatives considered and rejected for v0.1: UUID namespaces (no human
meaning, no discovery), public-key namespaces / DIDs (excellent properties,
poor ergonomics and ecosystem maturity — revisit post-1.0). The design keeps
the door open: authorities are syntactically self-describing, so a future
`murl://ed25519:…/name` form could be added without breaking existing names.

Resolvers MUST treat `local` as reserved. The authorities `invalid`,
`example`, `test`, and `localhost` behave per their DNS/RFC 2606 semantics.

## 5. The manifest

### 5.1 Envelope

* Media type: `application/murl+json` (registration planned; consumers
  SHOULD also accept `application/json`). File extension: `.murl.json`.
* Encoding: UTF-8 JSON. The top-level value MUST be an object.
* Size: a resolver MUST enforce a byte limit *before* parsing. Default
  262 144 bytes.
* Numbers anywhere in a manifest MUST be integers (see §7.1).
* Unknown members MUST be ignored by v0.1 consumers (forward compatibility)
  and SHOULD be surfaced as warnings. Signatures cover unknown members (§7).

### 5.2 Top-level members

| Member | Type | Req | Meaning |
|---|---|---|---|
| `murlVersion` | string | yes | Spec version. `"0.1"`. |
| `id` | string | no* | The canonical mURL this manifest is bound to (§6.4). *SHOULD be present in signed manifests.* |
| `name` | string | yes | Human name of the destination. 1–120 chars, no control chars. |
| `description` | string | no | ≤ 2000 chars. |
| `version` | string | no | Content version, dotted integers (`"1.4.2"`). Never `latest`. |
| `expires` | string | no | Strict UTC timestamp `YYYY-MM-DDTHH:MM:SSZ`. After this instant the manifest is expired (§8.4). |
| `resources` | array | yes | 1–64 resource objects. |
| `relations` | array | no | ≤128 typed metadata edges (§5.4). |
| `signature` | object | no | Detached signature block (§7.2). |

### 5.3 Resource members

| Member | Type | Req | Meaning |
|---|---|---|---|
| `id` | string | yes | `[a-z0-9][a-z0-9_-]{0,63}`, unique within the manifest. Selector-addressable. |
| `kind` | string | yes | `https`, `file`, `dir`, `murl`, `terminal`, or `custom:<name>` with `<name>` = `[a-z0-9][a-z0-9_-]{0,31}`. |
| `target` | string | yes | Kind-specific, 1–2048 bytes, no control characters. See below. |
| `label` | string | no | Human label, 1–120 chars. |
| `role` | string | no | Semantic role, `[a-z0-9][a-z0-9-]{0,31}`. Open vocabulary; `source`, `docs`, `issues`, `monitoring`, `workspace` are conventional. |
| `required` | bool | no | Default `false`. A required resource that fails to open fails the whole activation (§6.7). |
| `order` | int | no | 0–10000, default 100. Launch ordering weight; lower first. |
| `dependsOn` | array | no | ≤16 resource ids that MUST dispatch before this one. The graph MUST be acyclic. |
| `tags` | array | no | ≤16 tags, `[a-z0-9-]{1,32}`. |
| `integrity` | string | no | `sha256-<base64>` pin over the raw bytes of a nested manifest. Only enforced for `kind: murl`. |
| `meta` | any | no | Opaque to the resolver. |

Target validation per kind (MUST):

* `https` — `https://` URL; `http://` only for loopback hosts
  (`localhost`, `127.0.0.1`, `*.localhost`). Userinfo in targets is
  forbidden. No spaces.
* `file`, `dir`, `terminal` — an absolute path (`/…` or `X:\…`/`X:/…`) or a
  `~`/`~/…` path. Dot segments (`.`/`..`) are forbidden. Relative paths are
  forbidden — there is no "relative to what?" answer that survives
  OS-handler activation.
* `murl` — a valid mURL. A selector on a nested mURL has no defined
  semantics in v0.1 and MUST be ignored with a warning.
* `custom:<name>` — free-form (charset/length rules only). Dispatched only
  through a handler the user registered locally; unregistered custom kinds
  never launch.

### 5.4 Relations

`{ "from": <id>, "rel": <[a-z][a-z-]{0,31}>, "to": <id> }` — typed edges
between resources. In v0.1 relations are **metadata only**: they carry
meaning for humans and future tooling (e.g. `documented-by`, `observes`,
`produces`) and MUST NOT affect resolution or dispatch. `dependsOn` is the
only edge type with runtime semantics (ordering). See
`docs/architecture.md`, "Graph, deliberately small".

## 6. Resolution

### 6.1 Pipeline

```text
murl string
  → parse                         (§3; reject on any violation)
  → locate manifest bytes         (§6.2, §6.3)
  → enforce size cap, parse JSON
  → validate                      (§5; reject on any error)
  → verify signature if present   (§7; INVALID signature = hard stop)
  → bind identity                 (§6.4)
  → splice nested mURLs           (§6.5, under limits §6.6)
  → classify + evaluate policy    (§8)
  → plan → consent → dispatch     (§8.3, §6.7)
```

### 6.2 Locating manifests

* `murl://local/<name>` — the local name store. Never the network.
* `murl://<host>[:port]/<name>[@ver]` — the **well-known mapping**:

```text
https://<host>[:port]/.well-known/murl/<name>.murl.json          (@latest)
https://<host>[:port]/.well-known/murl/<name>@<ver>.murl.json    (pinned)
```

Name segments are percent-encoded in the URL exactly as in the mURL
canonical form. Loopback authorities MAY use plain `http`.

Fetch requirements (MUST): TLS for non-loopback; **zero redirects** (a
manifest lives where its authority says it lives — following redirects would
let one authority serve another's namespace); size cap enforced while
reading; per-fetch timeout. Resolvers SHOULD refuse to fetch from hosts that
resolve only to private, link-local, or loopback address ranges (SSRF; see
`docs/threat-model.md` T-10).

### 6.3 Caching and offline behavior

* Fetched manifests are cached keyed by identity, with the source URL,
  fetch time, and a content hash; the hash MUST be verified on read.
* `@latest` entries are fresh for a TTL (default 3600 s). Pinned-version
  entries are immutable and never stale.
* Fresh cache → used silently. Stale cache + failed fetch → MAY be used
  with a surfaced warning. Offline + any cache → MAY be used with a
  warning. Offline + no cache → resolution fails.

### 6.4 Identity binding

If a manifest carries `id`, and it was resolved *by name*, the resolver MUST
check: same authority, same name; and if both the request and the `id` pin a
version, the same version. On mismatch, resolution MUST fail.

This prevents **re-labeling**: a validly signed manifest for
`murl://corp.example/harmless` served under the name
`murl://corp.example/payroll` is refused even though its signature verifies.
Signed manifests SHOULD therefore always carry `id`; resolvers SHOULD warn
when one does not.

### 6.5 Recursive mURLs

A `murl`-kind resource names another destination whose resources are
**spliced** into the plan at its position. The container resource itself is
never dispatched. Nested manifests are resolved with the same pipeline
(including trust evaluation, per manifest).

* **Cycles** (a nested identity already on the current resolution path) are
  a hard error.
* A nested identity already resolved elsewhere in the tree (a DAG diamond)
  is skipped with a warning — its resources are already in the plan.
* Duplicate `(kind, target)` pairs across the whole plan are skipped with a
  warning.
* Crossing authorities (parent and child under different authorities) is
  permitted but MUST be surfaced as a warning.
* An `integrity` pin on the container MUST be checked against the raw bytes
  of the nested manifest before parsing it.

### 6.6 Limits (normative defaults)

| Limit | Default | On breach |
|---|---|---|
| Nesting depth (root = 0) | 3 | hard error |
| Total dispatchable resources | 64 | hard error |
| Manifests per resolution | 8 | hard error |
| Manifest size | 262 144 B | hard error |
| Fetch timeout | 10 s | fetch error |
| Redirects | 0 | fetch error |
| Cache TTL (`@latest`) | 3600 s | staleness |
| Dispatch stagger | 150 ms | — |

Implementations MAY let users *tighten* these. Loosening them is a local
policy decision and MUST never be triggerable by manifest content.

### 6.7 Selectors and failure semantics

`#selector` addresses one resource id **in the root manifest**. Resolution
keeps only that resource — including, for a `murl`-kind resource, its
spliced children. A selector matching nothing is an error, not an empty
success.

Per-resource outcomes: `OPENED`, `SKIPPED`, `DENIED`, `FAILED`,
`UNAVAILABLE`. Aggregate:

| Aggregate | Condition |
|---|---|
| `SUCCESS` | every non-skipped resource opened |
| `PARTIAL_SUCCESS` | some opened; no `required` resource missed |
| `FAILED` | a `required` resource did not open, or nothing opened and something failed |
| `DENIED` | nothing opened because everything was denied |

A destination is a set; one dead dashboard must not take down the other
seven resources.

## 7. Canonical form and signatures

### 7.1 MCF-1 (mURL Canonical Form 1)

The byte form that hashes and signatures cover:

* Object members sorted by Unicode code point of the member name.
* No insignificant whitespace.
* String escaping: `\" \\ \b \f \n \r \t`, `\u00XX` for other control
  characters; all other characters as raw UTF-8.
* Numbers MUST be integers within `i64`/`u64`, emitted in plain decimal.
  Non-integer numbers are invalid in manifests.

MCF-1 output is byte-identical to RFC 8785 (JCS) for every document the
manifest schema allows. It deliberately rejects floats — the hardest part of
JCS to reimplement identically — because a canonical form that is easy to
get wrong produces signatures that are easy to break accidentally.

### 7.2 Signature block

```json
"signature": {
  "alg": "ed25519",
  "keyId": "ed25519:<first 16 hex of sha256(publicKey)>",
  "publicKey": "<base64, 32 bytes>",
  "sig": "<base64, 64 bytes>"
}
```

Signing: remove any `signature` member, canonicalize (MCF-1), sign with
ed25519, insert the block. Verification recomputes the same bytes. The
`keyId` MUST match the embedded `publicKey` (derivable, so it cannot lie).

A present-but-invalid signature is a **hard stop** for the whole resolution
— it is evidence of tampering, not an unsigned manifest.

A valid signature proves only *continuity of authorship*. Trust is separate
and local: users pin public keys per authority (`docs/trust-model.md`).
Trust states: `LOCAL`, `UNSIGNED`, `SIGNED` (unknown key), `TRUSTED`
(pinned key).

## 8. Security model (summary; normative detail in docs/security.md)

### 8.1 Principles

1. **Manifests propose; local policy disposes.** A manifest carries no
   permissions and cannot grant itself anything.
2. **Consent before dispatch.** The user sees the plan — every resource,
   its tier, its origin, its trust — before anything opens.
3. **No shell, ever.** Dispatch is argv arrays via the process API.
   Targets are data.
4. **Fail closed.** No TTY and no explicit flags means no consent means no
   dispatch.

### 8.2 Tiers

| Tier | Kinds | Worst case |
|---|---|---|
| SAFE | `https` | a browser tab |
| SENSITIVE | `file`, `dir` (non-executable) | local data exposure |
| DANGEROUS | `terminal`, `custom:*`, `file` with an executable extension (`.exe`, `.sh`, `.desktop`, `.lnk`, …) | code execution |

"Opening" an executable or a `.desktop` file *is running it*; classification
MUST reflect that.

### 8.3 Policy baseline (defaults)

* Every tier requires consent (prompt) by default.
* DANGEROUS additionally requires the manifest to be trusted (`LOCAL` or
  `TRUSTED`). An untrusted manifest's DANGEROUS resources are **denied**,
  not prompted — a user can be talked into one click, and one click is
  exactly what a hostile mURL gets.
* A remotely fetched manifest referencing the local filesystem adds a
  consent reason (a remote author asserting knowledge of your disk is
  suspicious).
* An expired manifest: SAFE resources prompt with a warning; everything
  else is denied.

## 9. Versioning

* **Spec version** (`murlVersion`): this document is `0.1`. Consumers MUST
  reject manifests with a different major version and SHOULD accept
  same-major/newer-minor with warnings (post-1.0 rule; pre-1.0 everything
  may break).
* **Name versions** (`@1.4.2`): pinned resolutions are immutable —
  authorities MUST NOT change the content behind a pinned version, and
  resolvers cache them indefinitely. `@latest` is a mutable alias.
* **Manifest `version`**: informational content version.

## 10. Registration considerations

* The `murl` scheme is unregistered. The intended path is **provisional
  registration** per RFC 7595 §5.2 once the format stabilizes. Until then
  this specification claims only the conventional `murl://` string.
* `application/murl+json` is likewise unregistered; the `+json` structured
  syntax follows RFC 6839.
* Known name collisions (unrelated single-URL libraries named "murl" on
  PyPI and crates.io) are documented in `docs/prior-art.md`; the scheme
  string itself has no known conflicting deployment.

## 11. Extensibility

* **Kinds**: `custom:<name>` is the extension point; dispatch requires
  explicit local handler registration. New built-in kinds require a spec
  revision.
* **Unknown manifest members**: ignored with warnings, covered by
  signatures.
* **Query component**: reserved. v0.1 resolvers preserve and ignore it.
* **Reserved for future versions**: resource-level `profiles`, presentation
  hints (layout), authority forms other than DNS/`local`.
