# mURL — Multi-Resource Uniform Locator

**One identifier that opens a whole working context.**

> Status: **experimental** (v0.1). A working reference implementation of a
> proposed primitive, not a standard. Interfaces and the format itself may
> change. See [docs/roadmap.md](docs/roadmap.md) for stability labels.

```text
URL   :  identifier ──────────────▶ one resource
mURL  :  identifier ──▶ manifest ──▶ { resource, resource, resource, … }
```

A URL answers *"where is this resource?"*. An mURL answers *"what collection
of resources constitutes this logical destination?"* — and lets the
operating system resolve it, show you exactly what that means, and dispatch
each part to the right handler:

```text
                    murl://acme.example/project-x
                                 │
                        ┌────────▼────────┐
                        │  mURL resolver   │  parse → fetch manifest →
                        │                  │  validate → verify → policy
                        └────────┬────────┘
                                 │  consent: "Project X wants to open…"
     ┌──────────┬──────────┬─────┴─────┬───────────┬──────────┐
     ▼          ▼          ▼           ▼           ▼          ▼
  GitHub      Docs       Jira       Grafana   ~/projects   terminal
     │          │          │           │       /project-x     │
     ▼          ▼          ▼           ▼           ▼          ▼
  browser    browser    browser     browser    file mgr   terminal app
   SAFE       SAFE       SAFE        SAFE     SENSITIVE   DANGEROUS
```

## Why this exists

The unit of human intent is a *destination* — "working on Project X",
"on-call for checkout", "onboarding week 1" — but the unit of addressing has
always been a single resource. The glue between them keeps being reinvented,
badly and tool-locally: bookmark folders, links-pages, tmuxinator configs,
PowerToys Workspaces, "useful links" wikis. None of them give you a **name**
you can put in a chat message that is *inspectable before it acts*,
*verifiable* (signed), *versionable* (`@1.4.2`), *composable* (destinations
inside destinations), and *safe to click* (consent + policy + trust).

mURL is that name, plus the machinery that makes it safe:

* **A name, never a container** — `murl://authority/name[@version][#part]`
  resolves through a local store or `https://…/.well-known/murl/…` to a
  JSON **manifest**. Publishing a namespace = static files on any web
  server. No registry, no accounts, no new infrastructure.
* **Security as part of the addressing layer** — resources are classified
  SAFE / SENSITIVE / DANGEROUS by what they *are*; everything needs
  consent; DANGEROUS additionally needs *trust* (local, or signed by a key
  you pinned). Untrusted terminals aren't a prompt — they're a refusal.
  No launch ever touches a shell. Limits (depth 3, 64 resources, 256 KiB,
  cycle detection) are hard errors.
* **Verifiable** — ed25519 signatures over a canonical form (MCF-1),
  per-authority key pinning, identity binding (a signed manifest can't be
  replayed under a different name), byte-exact integrity pins for nested
  destinations.

## Quick start

```bash
git clone <this-repo> && cd mURL
cargo build

# the full guided tour (hermetic — temp state, dry-run open, auto-cleanup):
bash examples/demo.sh
```

Or by hand:

```bash
murl create --name "Project X"                # write a starter manifest
murl validate project-x.murl.json
murl name add project-x project-x.murl.json   # install as murl://local/project-x
murl resolve murl://local/project-x           # see the full plan — nothing opens
murl open murl://local/project-x              # consent, then dispatch
murl open 'murl://local/project-x#monitoring' # just one part of it

murl keygen && murl sign project-x.murl.json  # sign it
murl os install                               # make murl:// clickable (Linux/Windows)
```

A manifest looks like this (full schema: [spec §5](spec/SPECIFICATION.md)):

```json
{
  "murlVersion": "0.1",
  "id": "murl://local/project-x",
  "name": "Project X",
  "resources": [
    { "id": "source",    "kind": "https",    "target": "https://github.com/acme/project-x", "role": "source" },
    { "id": "docs",      "kind": "https",    "target": "https://docs.acme.example/x",       "role": "docs" },
    { "id": "workspace", "kind": "dir",      "target": "~/projects/project-x",              "role": "workspace" },
    { "id": "term",      "kind": "terminal", "target": "~/projects/project-x", "dependsOn": ["workspace"] },
    { "id": "team",      "kind": "murl",     "target": "murl://local/team" }
  ]
}
```

## Documentation

| | |
|---|---|
| [docs/concept.md](docs/concept.md) | why mURL exists; what it is and is not |
| [spec/SPECIFICATION.md](spec/SPECIFICATION.md) | **the normative v0.1 specification** |
| [docs/architecture.md](docs/architecture.md) | crate layout, pipeline, decisions & reasons |
| [docs/security.md](docs/security.md) · [docs/threat-model.md](docs/threat-model.md) | the security model and the 16 threats it answers |
| [docs/trust-model.md](docs/trust-model.md) | signatures, pinning, and why not PKI |
| [docs/resolution.md](docs/resolution.md) · [docs/resource-types.md](docs/resource-types.md) | how names resolve; the kind registry |
| [docs/os-integration.md](docs/os-integration.md) | Linux/Windows registration; macOS plan |
| [docs/prior-art.md](docs/prior-art.md) | honest map of neighbors (ORE, Metalink, PowerToys, …) and what's actually novel |
| [docs/examples.md](docs/examples.md) · [docs/faq.md](docs/faq.md) · [docs/roadmap.md](docs/roadmap.md) | walkthroughs, answers, plans |

## Repository layout

```text
crates/murl-core   the protocol: parser · manifest · validator · resolver ·
                   policy · trust · cache · dispatch planning (no I/O effects;
                   network & process creation live behind traits)
crates/murl-cli    the `murl` binary: commands, HTTPS fetcher (SSRF-guarded),
                   launcher (argv-only), consent UI, OS registration
fuzz/              cargo-fuzz targets: parser, manifest, canonical JSON
spec/              the formal specification
docs/              design & operations documentation
examples/          Project X demo destination + demo.sh
```

## Backward compatibility

mURL does not replace URLs — every leaf target *is* an ordinary URL or path,
dispatched to the same browser and apps as today. It is a composition layer
above locators. `https://` links keep working; an mURL just gives a set of
them (plus local resources) a shared, verifiable name.

## Development

```bash
cargo test                 # 120+ unit/integration/security tests
cargo clippy --all-targets # lint-clean, unsafe_code forbidden workspace-wide
cargo fmt --check
cargo +nightly fuzz run parse_murl   # needs cargo-fuzz
```

Contributions: see [CONTRIBUTING.md](CONTRIBUTING.md). Security reports:
[SECURITY.md](SECURITY.md). Licensed under [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE), at your option.
