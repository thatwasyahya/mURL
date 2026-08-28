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

## v0.2 — format hardening ✅ shipped

Focus: make the *format* trustworthy enough to freeze.

- [x] Reject duplicate JSON members outright, at every nesting level
      (threat T-15 residual closed) — strict parser in `murl-core::json`
- [x] `notBefore` validity floor; integer-only enforcement everywhere
      including `meta`; spec text for `@latest` mutability and rollback
- [x] Selector extensions: multi-item `#a,b`, `#role=docs`, `#tag=dev`,
      union semantics, every item must match
- [x] Manifest JSON Schema (`spec/murl-manifest.schema.json`) and the
      conformance suite (`spec/conformance/`: 14 valid + 34 invalid
      manifests, 79 mURL vectors) with a Rust harness other
      implementations can copy
- [x] `murl create --interactive`; `murl export`/`import` bundles
      (verbatim bytes + integrity, imports land in the local namespace)

## v0.3 — the daemon and real consent UX ✅ shipped

Focus: replace the terminal consent compromise.

- [x] `murl-daemon`: user-private socket, single-instance (refuses to
      clobber a live socket), transport-free request handler; the CLI is a
      thin client with `--daemon`/`--no-daemon` and silent fallback
- [x] IPC threat model (D-1 … D-7) written **before** the implementation,
      as the gate required — `docs/daemon.md`
- [x] `ConsentUi` abstraction with a terminal implementation; a GUI drops
      in without touching the protocol or the security model. A test drives
      a deliberately rogue surface to prove policy denials survive it
- [x] `murl-net` extracted so CLI and daemon share one hardened fetcher
- [ ] Native GTK/portal dialog and Windows named-pipe transport (the
      remaining surface work; the abstraction and protocol are in place)
- [ ] Background cache refresh + `expires` notifications

## v0.4 — platform completeness ✅ shipped (except notarization)

- [x] macOS app-bundle registration: `packaging/macos/build-app.sh` +
      `Info.plist.in` produce `mURL.app`, the only way Launch Services will
      accept a scheme claim
- [x] New built-in kinds, each with a security review and conformance
      vectors: `ssh` (DANGEROUS, handler-gated, option-smuggling refused),
      `remote-desktop` (DANGEROUS, no userinfo), `geo` (SAFE,
      range-checked), `mailto` (SAFE, header allow-list)
- [ ] Notarized macOS release artifacts (needs an Apple developer identity)
- [ ] Handler discovery quality-of-life (detect common terminals; still
      explicit opt-in)
- [ ] Localization of consent surfaces

## v1.0 — freeze and register

**Gate (unchanged and deliberately not met): at least one non-reference
implementation passing `spec/conformance/`, and real usage feedback on the
consent model.** Everything that does not require those is done:

- [x] Compatibility policy and stability labels: `docs/stability.md`,
      with `#[non_exhaustive]` on the enums it commits to growing
- [x] IANA registration templates written in full and **not submitted**:
      `spec/registration/` (RFC 7595 provisional scheme, RFC 6838/6839
      media type). Registering a scheme claims a global namespace; doing it
      before anyone else has implemented the format would be the wrong
      order, and each template says so.
- [x] Conformance suite + schema check wired into CI
- [ ] Spec 1.0 freeze — waits on the gate
- [ ] Submit the registrations — waits on the gate

## Review findings folded in (2026-08-28)

An adversarial review of the v0.2–v1.0-prep work (five reviewers, each
finding then faced with two skeptics told to refute it) produced eleven real
defects, all now fixed with regressions:

* the daemon ignored the user's `config.json`/`handlers.json`, so a
  configured `"dangerous": "deny"` became a clickable prompt (T-18);
* `--offline` was silently dropped when routing through the daemon — a
  fail-**open**;
* four bundle import defects (silent name collisions, decoded segments
  re-parsed as grammar, missing identity binding, unreachable `--as`);
* argument injection through the Windows OS handler, since the activated
  URL is substituted into a command template before argv is parsed (T-17);
* tilde paths escaping the home directory via `Path::join`, empty handler
  templates executing the target as a program, Windows trailing-dot
  executable classification, and an unbounded daemon read.

Worth recording *why* they existed: every one lived in a seam between two
components that each looked correct alone. That is the class of defect this
project should keep spending review budget on.

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
