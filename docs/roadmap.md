# Roadmap

Stability labels used below and throughout the project:

* **experimental** — may change or vanish without notice (everything today)
* **stable** — changes only with a spec version bump
* **implementation-specific** — behavior of this codebase, not the format

## v0.1 — the primitive (this release)

Prove: *one stable identifier can represent and safely resolve a collection
of heterogeneous resources.*

- [x] `murl://` grammar + hardened parser (fuzzed)
- [x] Manifest format + validator (fuzzed) · MCF-1 canonical form
- [x] Resolver: local store, HTTPS well-known, cache + offline fallback,
      recursion with cycle/depth/count limits, identity binding
- [x] Security: tier classification, consent policy, trust-gated DANGEROUS,
      SSRF filtering, no-shell dispatch
- [x] Trust: ed25519 sign/verify, per-authority key pinning, integrity pins
- [x] CLI: parse · create · validate · inspect · resolve · open · name ·
      keygen · sign · verify · trust · cache · os · handler
- [x] OS registration: Linux (XDG), Windows (HKCU); macOS documented stub
- [x] 120+ tests incl. security suites · 3 fuzz targets · CI · deny.toml

## v0.2 — format hardening

Focus: make the *format* trustworthy enough to freeze.

- [ ] Reject duplicate JSON keys outright (threat T-15 residual)
- [ ] `expires`/rollback guidance; `not-before`; spec text for `@latest`
      mutability contracts
- [ ] Selector extensions: `#role=docs`, multi-select semantics
- [ ] Manifest JSON Schema published alongside the spec; conformance test
      vectors (valid/invalid corpora) other implementations can run
- [ ] `murl create` interactive mode; `murl export`/`import` (bundle a
      name + nested manifests + pins into one shareable file)
- [ ] Windows/macOS CI packaging artifacts (signed archives)

## v0.3 — the daemon and real consent UX

Focus: replace the terminal consent compromise.

- [ ] `murl-daemon`: single instance, socket-activated; CLI and OS handler
      become thin clients (core crate unchanged — that's why it's split)
- [ ] Native consent dialog (GTK/portal on Linux; Windows dialog), showing
      the plan with tier grouping and per-resource toggles
- [ ] Background cache refresh + `expires` notifications
- [ ] D-Bus service on Linux; named-pipe IPC on Windows — IPC surface gets
      its own threat-model chapter *before* implementation

## v0.4 — platform completeness

- [ ] macOS app-bundle registration (`mURL.app` wrapping the CLI) + notarized
      release artifacts
- [ ] New built-in kind candidates, each with a security review: `ssh`
      (DANGEROUS), `rdp`/`vnc` (DANGEROUS), `geo`, `mailto`-equivalent
- [ ] Handler discovery quality-of-life (detect common terminals; still
      explicit opt-in)
- [ ] Localization of consent surfaces

## v1.0 — freeze and register

Gate: at least one non-reference implementation consuming the conformance
vectors, and real-world usage feedback on the consent model.

- [ ] Spec 1.0: stable grammar, manifest, resolution, trust; MUST-level
      conformance statements complete
- [ ] IANA provisional scheme registration (`murl`, RFC 7595 §5.2) and
      `application/murl+json` media-type registration
- [ ] Semantic-versioning commitment for murl-core API

## Explicitly deferred (with reasons)

* **Presentation/layout hints** (Layer 5): valuable, but binding the
  addressing format to window management this early would couple the spec
  to desktops that disagree with each other. Design constraint recorded:
  it must arrive as a separate, ignorable manifest section.
* **Rich graph semantics** (queries, relation-driven activation):
  `relations` carries the data today; semantics wait for a consumer.
* **Transparency log / monotonic versions for `@latest`** (threat T-16):
  correct answer known, cost unjustified before adoption.
* **Decentralized namespaces (DID/public-key authorities)**: grammar space
  reserved; revisit when the ecosystem can hold keys for humans.
* **Blockchain anything**: no problem here needs it.
