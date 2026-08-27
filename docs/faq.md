# FAQ

**Isn't this just "open my bookmarks folder"?**
A bookmarks folder is browser-local, URL-only, unversioned, unsigned, and
unaddressable from outside the browser. An mURL is a *name* that works
anywhere a link works, resolves to a set including non-web resources
(directories, terminals, custom tools), can be pinned/signed/verified, and
passes through a consent and policy engine before anything opens. The
overlap is the four `https` resources; everything else is the point.

**Why a new scheme instead of an `https://` page of links?**
A link page delegates *interpretation* to a human ("click these eight
things") and cannot express local resources at all. A scheme gives the OS a
machine-interpretable destination: one activation, one consent surface, one
plan — including `dir` and `terminal` kinds no web page may touch.

**Why does the mURL not contain the resources directly?**
Identifier-embeds-content fails every requirement that matters: length
limits break sharing, the set can't change without changing the name,
there's nothing stable to sign, nothing to cache, nothing to version.
`data:` URIs are the cautionary precedent. mURL is a name; the manifest is
the content (spec §1).

**Why JSON and not YAML/TOML/RDF?**
YAML's implicit typing and complexity are a poor fit for a
security-validated format; TOML handles nesting awkwardly; RDF (ORE's
choice) buys expressiveness nobody consumes at the cost of every
implementer. JSON has universal parsers, a canonicalization story (JCS,
restricted here to MCF-1), and is boring — boring is a security feature.
CBOR is a plausible future *encoding* of the same model.

**Why is `terminal` even in v0.1 if it's so dangerous?**
Because a resource type the model can't express safely would just be
smuggled through weaker channels (a `.sh` file, a `custom:` kind). Putting
it in-model forces the honest answer: DANGEROUS tier, trust-gated, consent
required, handler explicitly configured. The security model is the feature.

**What happens when a resource is unavailable?**
The rest of the destination still opens. Per-resource outcomes roll up to
SUCCESS / PARTIAL_SUCCESS / FAILED / DENIED (required resources can fail
the whole activation). See spec §6.7.

**Can an mURL contain another mURL? What stops infinite recursion?**
Yes — `kind: murl` splices the child destination. Depth ≤ 3, ≤ 8 manifests,
≤ 64 resources, cycle detection on the resolution path, duplicate
suppression, optional byte-exact integrity pins. All hard errors, none
overridable by manifest content (spec §6.5–6.6).

**Why DNS for namespaces instead of something decentralized?**
DNS+TLS is the one namespace with universal deployment, existing ownership
semantics, and existing operational practice. UUIDs lose human meaning;
DIDs/public-key namespaces are stronger but eco-immature — the authority
grammar leaves room to add them later without breaking names (spec §4).
And `local` exists precisely so no network authority is *required*.

**Does mURL replace URLs?**
No, structurally: every leaf target *is* a URL or path. mURL is a
composition layer above locators, not a competitor to them (README,
"Backward compatibility").

**Why no daemon? / Why is consent a terminal prompt?**
v0.1 ships the smallest thing whose security story is complete: single-shot
CLI, no IPC surface, no resident process. The daemon (native consent
dialogs, background refresh, single-instance activation) is v0.3, and the
core/CLI split means it reuses the pipeline unchanged
(docs/architecture.md).

**How do I stop trusting someone?**
`murl trust remove <authority> <keyId>` — or delete the pin from
`trust.json`. Trust is one auditable local file (docs/trust-model.md).

**Windows/macOS support?**
Core, CLI, dispatch, and tests are cross-platform (CI runs all three).
Scheme *registration* is implemented on Linux and Windows; macOS needs an
app bundle (documented stub, roadmap v0.4) — a bare binary cannot claim a
scheme with Launch Services.

**Is "mURL" related to the `murl` packages on PyPI/crates.io?**
No — those are single-URL manipulation libraries (docs/prior-art.md, "Name
collisions"). Publishable crate names here are `murl-core`/`murl-cli`.

**Is this a standard?**
No. It is an experimental format with one reference implementation and a
written spec. The honest path to "standard" is documented (spec §10:
RFC 7595 provisional registration; docs/roadmap.md) and starts with other
people finding it useful.
