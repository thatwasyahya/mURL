# Prior Art

An honest map of the neighborhood. The finding up front: **every ingredient
of mURL exists somewhere; the composition does not.** If a reader knows of a
system that already provides an OS-level, transport-agnostic identifier that
resolves to a signed manifest of heterogeneous resources with a consent
model — please open an issue; that discovery would change this project's
purpose from "build it" to "adopt it".

## Closest conceptual relatives

### OAI-ORE (Open Archives Initiative — Object Reuse and Exchange)

The strongest academic precedent: **aggregations of web resources given
identity** through a "Resource Map" describing a compound digital object.
*Overlap*: identity-for-a-set, a machine-readable map, typed relations.
*Difference*: ORE is descriptive metadata for scholarly/archival
interchange (RDF-based); it has no resolution-to-action pipeline, no OS
dispatch, no security or consent model, no local resources (terminals,
directories). *Reuse*: the aggregation-with-identity idea, consciously;
`relations` echoes ORE's typed relationships.

### RO-Crate / BagIt / OCFL

Research-object packaging: a directory + `ro-crate-metadata.json`
describing constituent files with rich provenance. *Overlap*:
manifest-describes-a-set, integrity, provenance ambitions. *Difference*:
packages *contain* their payload and travel as archives; an mURL *names*
live, heterogeneous, actionable resources it does not contain. *Reuse*: the
"manifest as first-class artifact" stance.

### Metalink (RFC 5854) — the inverse

Many URLs → **one** resource (mirrors/hashes for a download). mURL is one
name → **many** resources. Structurally the mirror image; noted to prevent
confusion. *Reuse*: SRI-style hash pinning is the same instinct as our
`integrity`.

### Web Bundles / WBN, and `data:` URIs

Bundle *representations* of resources into one blob for offline delivery.
*Difference*: bundling is containment; mURL rejects containment as a core
principle (spec §1) — the destination includes things that cannot be
contained (a terminal, a directory, a live dashboard). Also the cautionary
tale for why identifiers must not embed content (length, staleness,
unauditability).

## Same itch, tool-local scratches

| System | What it does | Why it isn't the primitive |
|---|---|---|
| **PowerToys Workspaces** | one click launches an app set with window layout (Windows) | GUI-captured local config; no identifier, no sharing-as-name, no manifest format, no trust model |
| tmuxinator / tmuxp | YAML → tmux session with windows/commands | terminal-only, local-only |
| VS Code `.code-workspace`, JetBrains projects | one file opens a multi-root workspace | one application's world |
| devcontainer.json | declarative dev environment | container tooling, not addressing |
| Browser session managers (Workona, Toby, OneTab, tab groups) | named sets of tabs | browser-only, proprietary sync, URLs only |
| Apple Shortcuts / Automator | user-authored multi-step automation | imperative program, not a declarative addressable set; sharing executes logic, which is the security anti-pattern mURL avoids |
| KDE Activities / virtual-desktop session restore | desktop-state recall | machine-local state, not an identifier |
| Nix flakes | one reference → reproducible derivation set | build-system semantics; though `flake:`-style refs and pinning were an inspiration for `@version` immutability |

These prove the *demand* (the pattern keeps being reinvented per-tool) and
the *gap* (none of them compose across tools).

## Infrastructure mURL deliberately reuses

| Mechanism | Where mURL uses it |
|---|---|
| RFC 3986 URI syntax | `murl://` follows generic syntax (with documented, security-motivated restrictions) so existing linkifiers/routers treat it as a URI |
| RFC 7595 provisional scheme registration | the stated registration path (spec §10) |
| RFC 8615 `/.well-known/` | remote manifest discovery — no registry needed |
| DNS + TLS | the namespace and its transport security (spec §4) |
| OS scheme-handler machinery (XDG `x-scheme-handler/*`, Windows `URL Protocol`, macOS Launch Services) | activation (docs/os-integration.md) |
| RFC 8785 JCS (restricted profile) | MCF-1 canonical form (spec §7.1) |
| SRI-style `sha256-…` | nested-manifest integrity pins |
| ed25519 (RFC 8032) | manifest signatures |
| Web App Manifest / app-manifest culture | precedent for "JSON manifest describes an installable-ish thing" |

## Name collisions (branding due diligence)

* **`murl` on PyPI** (berkerpeksag/murl) and **`murl` on crates.io**
  (Tomaz-Vieira/murl): both are single-URL *manipulation libraries* —
  unrelated in concept, but they own the package names. Consequence: if
  these crates are ever published, they go out as `murl-core`/`murl-cli`
  (already their names); the CLI binary keeping the name `murl` conflicts
  with nothing on crates.io (binary names are not registry-scoped).
* The `murl://` **scheme string** has no known deployment (searched: IANA
  registries, W3C UriSchemes wiki, general web). "mURL" as shorthand for
  "misspelled curl" appears informally; not a technical conflict.
* Expansion: **Multi-Resource Uniform Locator** (not "Multi-URL") — the
  set members are not all URLs (terminals, custom kinds), so "multi-URL"
  would misdescribe the semantics. This matches the project's usage
  throughout.

## What is actually novel here

Kept deliberately narrow:

1. The **composition**: OS-level scheme → name → signed manifest →
   policy-gated dispatch of *heterogeneous* (web + filesystem + process)
   resources, specified as an open format.
2. The **security integration**: tier classification with
   executable-extension escalation, trust-gated DANGEROUS dispatch,
   identity binding against signature replay, SSRF-filtered resolution —
   *as properties of the addressing layer itself*, not of one application.
3. Small mechanical contributions: MCF-1 (a JCS restriction that trades
   float support for cross-implementation safety), and the
   userinfo-forbidden / ASCII-only identifier grammar as an anti-phishing
   stance.

Everything else is honest reuse, and the project should be judged on
whether the composition earns its existence — see the conclusion of
docs/faq.md.
