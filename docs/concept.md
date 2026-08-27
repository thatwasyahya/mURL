# The mURL Concept

## The gap

Every URL answers one question: *where is this resource?* But almost nothing
people actually do happens against one resource. "Working on Project X"
means a repository, a docs site, an issue tracker, a dashboard, a local
checkout, a terminal in that checkout. "Onboarding a new teammate" means
fifteen links and three local tools. The *unit of intent* is a destination;
the unit of addressing is a resource. Everything between those two is glue:
bookmark folders, onboarding wikis, "useful links" READMEs, tmuxinator
configs, PowerToys Workspaces — each tool-local, each unshareable as a
single name, each unverifiable.

mURL proposes the missing primitive:

```text
URL   :  identifier ──────────────▶ resource
mURL  :  identifier ──▶ manifest ──▶ { resource, resource, resource, … }
```

```text
                     murl://acme.example/project-x
                                  │
                          resolve manifest
                                  │
        ┌──────────┬──────────┬───┴──────┬───────────┬──────────┐
        ▼          ▼          ▼          ▼           ▼          ▼
     GitHub      Docs       Jira      Grafana    ~/projects   terminal
        │          │          │          │        /project-x     │
        ▼          ▼          ▼          ▼           ▼           ▼
     browser    browser    browser    browser    file mgr    terminal app
```

## What an mURL is (and is not)

An mURL is a **name**. It never contains resources; it resolves — through a
local store or an HTTPS well-known endpoint — to a **manifest** that
describes the resource set. That indirection is what buys every property
that matters:

| Property | Why the manifest indirection provides it |
|---|---|
| Stable sharing | the name survives while the set evolves |
| Verifiability | a document has canonical bytes; canonical bytes can be signed |
| Consent | the set can be *shown* to the user before anything opens |
| Caching/offline | a document can be cached with integrity; a behavior cannot |
| Versioning | `@1.4.2` pins immutable content; `@latest` floats |
| Composition | a manifest can reference another mURL (bounded, cycle-checked) |

An mURL is **not**:

* a "multi-tab opener" — browser tabs are one kind among many; local
  directories, terminals, and extensible custom kinds are peers;
* a workspace/session file — those are tool-local and unaddressable; an
  mURL is OS-level and shareable as a string;
* a replacement for URLs — every leaf target *is* an ordinary URL or path;
  mURL is a composition layer above them, not a successor.

## Why the OS level

Making `murl://…` an OS-registered scheme means the name works everywhere a
link works today: chat messages, email, documentation, QR codes, shell
commands. Click it, and the OS hands it to the resolver; the resolver shows
you what the destination is made of; you approve; each resource goes to its
proper handler. No browser extension, no per-app integration.

That power is exactly why security is the center of the design rather than
an afterthought: a link that can open twenty things, touch the filesystem,
and start terminals is an attack primitive unless consent, classification,
trust, and hard limits are built into the resolution pipeline itself. See
`docs/security.md` and `docs/threat-model.md`.

## The five layers

The protocol is deliberately layered so that the addressing core stays tiny
and GUI/desktop concerns can never leak into it:

```text
Layer 1  Addressing     murl:// syntax, authorities, versions      (spec §3–4)
Layer 2  Resolution     manifest location, caching, recursion      (spec §6)
Layer 3  Security       tiers, policy, trust, limits               (spec §7–8)
Layer 4  Dispatch       kind → handler → argv → process            (spec §8)
Layer 5  Presentation   layout/window hints — NOT in v0.1, and
                        never a dependency of layers 1–4
```

## Honest framing

Most ingredients here exist elsewhere: aggregation-with-identity (OAI-ORE,
RO-Crate), well-known discovery (RFC 8615), manifest signing (Sigstore et
al.), app-set launching (PowerToys Workspaces), scheme handlers (every OS).
What does not exist is the *composition*: one OS-level, transport-agnostic
identifier → signed manifest → policy-gated multi-resource dispatch, as an
open format with a security model. The claim mURL makes is that this
composition is a coherent, buildable primitive — and this repository is the
existence proof. `docs/prior-art.md` maps the neighbors precisely.
