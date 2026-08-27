# Threat Model

Scope: a user activates mURLs that may come from anywhere (chat, email, web,
QR codes); manifests may be authored by adversaries; the network between
resolver and authority may be adversarial. Out of scope: an attacker with
write access to the user's account/filesystem, and the behavior of handler
applications after launch (see docs/security.md, "edges").

Assets: the user's machine (code execution), local data (files/dirs), local
network (SSRF), attention/trust (phishing), and availability (resource
exhaustion).

Legend: ✅ mitigated in v0.1 · ⚠ partially · 📋 planned.

---

### T-1 · Hostile mURL string (parser attack surface) ✅

Malformed, oversized, or encoding-abusing identifiers aiming at parser bugs
or downstream injection.

**Defenses**: 1 KiB cap before any work; printable-ASCII-only; strict
percent-decoding (invalid/truncated escapes fatal); decoded segments checked
for UTF-8, control chars, separators, dot segments; total parse function —
no panics (fuzz target `parse_murl`, round-trip property).

### T-2 · Phishing via identifier confusion ⚠

`murl://github.com@evil.example/x`, homoglyph names, look-alike authorities.

**Defenses**: userinfo forbidden at grammar level; raw non-ASCII rejected
(homoglyphs must survive percent-encoding and are shown decoded only in
labeled contexts); authorities lowercase ASCII/punycode. **Residual**:
`gith?b-docs.example`-style look-alike *domains* are as phishable as on the
web; the consent plan showing the authority is the human-side mitigation.

### T-3 · Malicious manifest content ✅

Injection via targets (`; rm -rf ~`, `$(…)`, `%0a`), control characters in
labels shown to users, relative/traversal paths, `file://` tricks.

**Defenses**: validator rejects control characters everywhere, relative
paths, dot segments, non-https web targets, userinfo in targets; dispatch is
argv-only with single-element substitution — there is no shell to inject
into; labels are printed, never evaluated.

### T-4 · "Just a file" that executes ✅

`file` resource pointing at `.desktop`, `.exe`, `.lnk`, `.sh`, `.hta`…
"Viewing" these via the platform opener is execution.

**Defenses**: executable-extension list escalates classification to
DANGEROUS → untrusted manifests get these denied outright; trusted ones
still prompt.

### T-5 · Dangerous kinds from strangers ✅

A chat-delivered mURL whose manifest includes `terminal` or `custom:*`
resources.

**Defenses**: DANGEROUS requires trust (local install or pinned signing
key) *and* consent; `custom:*` additionally requires a locally registered
handler. All three are deliberate user acts an attacker cannot perform
remotely.

### T-6 · Consent fatigue / overwhelm ⚠

Burying one terminal among 60 checkboxes; repeated prompting until the user
clicks through.

**Defenses**: tier-grouped plan display with explicit reasons; DANGEROUS
never bundled into "approve all" for untrusted sources (denied before the
prompt); duplicate suppression shrinks the list. **Residual**: consent UX is
terminal-grade in v0.1; a native reviewed dialog is roadmap (daemon).

### T-7 · Resource explosion (zip-bomb-shaped) ✅

Huge manifests, 10 000 resources, deep JSON, giant strings.

**Defenses**: 256 KiB pre-parse size cap (enforced while reading from the
network); ≤ 64 resources per manifest and per resolution; field length caps
everywhere; serde_json recursion limit intact.

### T-8 · Recursive mURL bombs ✅

`a → b → a` cycles; diamond fan-out; 50-deep chains; nested manifests each
maximal.

**Defenses**: identity-stack cycle detection (hard error); visited-set
diamond dedup; depth ≤ 3; ≤ 8 manifests per resolution; ≤ 64 total
resources — whichever trips first stops everything. Tests:
`resolver_security.rs`.

### T-9 · Manifest substitution / re-labeling ✅

Authority (or MITM, or a signed-manifest replay) serves manifest X under
name Y — e.g. a benign signed manifest served as `payroll`.

**Defenses**: TLS + zero redirects; identity binding (`id` must match the
requested name, hard error); signed manifests warned when `id` missing;
integrity pins for nested manifests; pinned versions cached immutably.

### T-10 · SSRF via resolution ✅

`murl://intranet-host/…` or a DNS name resolving to 169.254.169.254 turning
a clicked link into a request inside the victim's network.

**Defenses**: DNS-time address filtering — non-loopback authorities
resolving only to private/link-local/loopback/CGNAT ranges are refused;
loopback plain-HTTP allowed solely for `localhost`/`127.0.0.1`/
`*.localhost` authorities (development); zero redirects prevents bounce-out.

### T-11 · Signature forgery / trust confusion ✅

Forged signatures, key substitution with a stale keyId, alg-confusion,
signature stripping.

**Defenses**: ed25519 only (no alg agility to confuse); keyId is *derived*
from the embedded key and checked; invalid signature = hard stop for the
entire resolution (tamper evidence, not "unsigned"); stripping a signature
downgrades trust to UNSIGNED, which forfeits DANGEROUS dispatch — the
attacker gains nothing they didn't already have. **Note**: verification uses
cofactorless `verify` (dalek); manifests are single-signer documents, so
malleability across verifiers is not load-bearing.

### T-12 · Cache poisoning ✅

Tampered cache files; entry crossing (identity A's bytes under identity B);
stale-forever behavior.

**Defenses**: per-entry sha256 verified on every read, mismatch = evict +
miss; identity recorded in metadata and checked; TTL for `@latest`; stale
use is loud (warning) and only as fallback.

### T-13 · Hostile handler registration 📋

Tricking the user into registering a malicious handler, or malware editing
`handlers.json`.

**Defenses today**: registration is explicitly local CLI action; manifests
have no channel to it; handler argv templates never pass through a shell.
**Residual/planned**: `handlers.json` is user-writable plain JSON (as is all
config — see out-of-scope); signed/attested handler registries are not
planned (out of proportion for a per-user tool).

### T-14 · Concurrency/launch flooding ✅

Even an approved plan acting as a fork bomb.

**Defenses**: sequential dispatch with 150 ms stagger; ≤ 64 resources;
required/optional semantics mean one hung handler doesn't wedge the plan
(spawn, don't wait).

### T-15 · Duplicate-key JSON differentials ✅ (closed in v0.2)

`{"target": "https://safe", "target": "file:///etc/passwd"}` interpreted
differently by verifier and consumer, or by two conformant implementations.

**Defenses**: duplicate object members are **invalid** (spec §5.1) and
rejected at parse time, at every nesting level, before a value exists —
`murl-core::json::from_slice_strict`. Bundles use the same strict parser.
The cross-implementation differential is closed at the format level, not
merely avoided by sharing one parser.

### T-16 · Offline/rollback attacks ⚠

Serving an old (validly signed) manifest; blocking fetches to force stale
cache; activating a pre-published manifest early.

**Defenses**: `expires` bounds staleness and `notBefore` bounds early
activation — outside either window, only SAFE resources reach a prompt and
everything else is denied (spec §8.3); stale-cache use is warned; pinned
versions are immutable by contract; spec §9 states the `@latest` mutability
contract and directs signing authorities to set `expires`. **Residual**: no
monotonic version enforcement for `@latest` (a transparency log remains out
of scope pre-1.0).
