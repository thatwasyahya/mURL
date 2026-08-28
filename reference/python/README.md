# murl_ref — a second implementation of the mURL format

A small, dependency-free Python implementation of the mURL **format**: the
`murl://` grammar (spec §3), manifest parsing and validation (spec §5), and
MCF-1, the canonical byte form (spec §7.1). It exists to answer one question:

> Is `spec/SPECIFICATION.md` complete enough to implement from?

It was written against the specification text, not by reading
`crates/murl-core`. Where the spec left a question open, the Rust reference is
authoritative — but every one of those moments is a gap in the prose, and they
are all listed at the bottom of this file. That list is the useful output of
the exercise; the code is mostly a way of generating it.

This is **not a usable mURL client**. It resolves nothing, opens nothing, and
verifies no signatures. See "What this deliberately omits".

## Status

Tracks format version **0.2** (accepts `0.1` manifests, per §9). Experimental,
like everything else here. Python 3.9+, standard library only.

## Running the conformance suite

```bash
cd reference/python
python3 run_conformance.py
```

It applies the four rules from [`spec/conformance/README.md`](../../spec/conformance/README.md)
to the vectors in `spec/conformance/`, prints a per-rule summary, and exits 0
on success or 1 on any failure. `--suite PATH` points it at a different copy of
the suite; `-v` lists every vector. Each rule asserts a minimum vector count
first, so a wrong path fails loudly instead of passing over an empty directory.

Current result:

```text
[PASS] rule 1: valid manifests parse and validate      16/16 vectors
[PASS] rule 2: invalid manifests are rejected          42/42 vectors
[PASS] rule 3: valid mURLs parse and round-trip        24/24 vectors
[PASS] rule 4: invalid mURLs are rejected              55/55 vectors

PASS: 137 vectors, 4/4 rules
```

## Using it directly

```python
from murl_ref import parse_murl, Manifest, signing_bytes

m = parse_murl("MURL://Example.COM/team/checkout@latest#role=docs")
m.canonical   # 'murl://example.com/team/checkout#role=docs'  (@latest elided)
m.identity    # 'murl://example.com/team/checkout'            (selector stripped)

manifest = Manifest.from_bytes(open("x.murl.json", "rb").read())
report = manifest.validate()
report.is_valid()   # bool: warnings never make it False
report.errors       # list[str]
report.warnings     # list[str] - unknown members, inert integrity pins, ...

signing_bytes(manifest.doc)   # the MCF-1 bytes a signature covers (§7.2)
```

Parsing raises (`MurlSyntaxError`, `ManifestError`); validation returns a
report. The split is deliberate: a malformed input has no useful partial
reading, but a schema-invalid manifest usually has several problems worth
showing an author at once.

## Layout

```text
murl_ref/parser.py     the murl:// grammar, canonical form, identity   (§3)
murl_ref/manifest.py   envelope + schema validation                    (§5)
murl_ref/canonical.py  MCF-1                                           (§7.1)
run_conformance.py     the four conformance rules, pass/fail, exit 0/1
```

Two implementation details worth knowing:

**Duplicate members are rejected at parse time**, at every nesting level, via
`json.load(object_pairs_hook=...)`. Python's default `json.loads` silently
keeps the *last* duplicate; another implementation might keep the first — and
then two consumers verify the same signature and act on different values
(§5.1, threat T-15). The hook is the only place this can be caught, because it
has to happen before any member is interpreted.

**Non-integer numbers are captured at parse time but reported at validation
time.** §5.1 says validators "MUST report any other number as an error", so a
float becomes a sentinel in the tree rather than aborting the parse. One stray
`1.5` then does not hide the rest of a document's problems.

## What this deliberately omits

Everything past the format. There is no resolver, no fetcher, no cache, no
policy engine, no dispatch, and no cryptography:

* **Resolution** (§6) — name stores, the `.well-known` mapping, redirect
  refusal, nesting and splicing, cycle detection, the §6.6 limits.
* **Dispatch** (§6.7, §8) — tiers, consent, trust, handlers, launching.
* **Signature verification** (§7.2) — the block's *shape* is validated
  (`alg`, `keyId` format, 32-byte key, 64-byte signature), and
  `signing_bytes()` produces exactly the bytes a signature covers, but no
  ed25519 runs. The standard library has none, and taking a dependency would
  defeat the point of a self-contained cross-check.
* **Time-of-use policy** (§8.3) — `notBefore` and `expires` are checked for
  format and ordering only. Validation here never reads the clock, the
  network, or the filesystem, exactly as `spec/conformance/README.md` requires.

The omissions are the point. Resolution and dispatch are where a *product*
lives; the format is where *interoperability* lives, and only the second one
needs a second implementation before it can be frozen.

## Spec questions this raised

In the order they were hit. Each is a place where the specification alone was
not enough to produce a conformant implementation — a conformance vector or
the reference implementation had to settle it. None of these are bugs in the
Rust; they are gaps in the prose.

1. **`murl://local:80/x` is rejected, but the grammar permits it.** §3.1 gives
   `authority = "local" / host [ ":" port ]`, and `local` is a syntactically
   valid `host`, so the second alternative matches `local:80` cleanly. Only
   the conformance vector reveals that the reserved word takes no port. The
   grammar needs an explicit exclusion, or §4 a sentence saying `local` is
   never a host.

2. **A valid vector contradicts §7.2's `keyId` MUST.** §7.2 says the `keyId`
   "MUST match the embedded `publicKey` (derivable, so it cannot lie)", but
   `manifests/valid/signature-shape.murl.json` pairs
   `ed25519:0123456789abcdef` with an all-zero key, whose actual key id is
   `ed25519:66687aadf862bd77`. Both are right, for different stages — the
   derivation check belongs to *verification* (§7), not *validation* (§5). But
   §5 never says which parts of the signature block a static validator must
   check, so an implementer has to guess, and guessing strictly turns a valid
   vector into a failure. §7.2 should say the derivation is checked at
   verification time.

3. **§7.1 does not say whether the hex in its unicode escapes is upper or
   lower case.** Signatures are byte-exact: an implementation that emits
   uppercase hex where the reference emits lowercase produces signatures
   nobody else can verify, from a two-character ambiguity. RFC 8785 mandates
   lowercase and §7.1 claims byte-identity with it, so lowercase is inferable
   — it should be stated outright.

4. **§7.1's escaping rule does not settle U+007F.** "...for other control
   characters" is silent on whether DEL counts. RFC 8785 escapes only below
   0x20 and the reference implementation agrees, so DEL is emitted raw — but
   "control character" is exactly the phrase under which someone would escape
   it. Same failure mode as (3), same fix: state the boundary as `< 0x20`.

5. **§7.1's byte-identity claim with RFC 8785 is stronger than the schema
   allows.** MCF-1 sorts member names by code point; JCS sorts by UTF-16 code
   unit. These disagree whenever a document holds both an astral member name
   and one in U+E000-U+FFFF — and the schema *does* permit arbitrary member
   names, inside `meta` and among the unknown members that forward
   compatibility requires. `canonical.rs` documents this restriction honestly;
   §7.1 asserts the identity without the caveat. (Verified: both
   implementations agree with each other, and both differ from JCS on such a
   document.)

6. **The conformance suite has no MCF-1 vectors at all.** Four directories
   cover the grammar and the manifest schema; nothing covers the canonical
   form, which is the one artifact where a single byte of disagreement breaks
   every signature between two implementations. This implementation's MCF-1
   could have passed all 137 vectors while being silently wrong — it had to be
   checked against `murl-core` out of band, which is exactly the work a vector
   suite exists to make unnecessary. A `canonical/` directory of input/output
   pairs (one per rule, plus the escape and integer edge cases) would close
   the largest remaining hole in the v1.0 gate.

7. **§5 does not say the manifest `id` must be an identity.** §5.2 calls `id`
   "the canonical mURL this manifest is bound to"; §3.3 defines identity as
   the canonical form *minus query and selector*; nothing joins the two.
   `manifests/invalid/id-with-selector.murl.json` says a selector in `id` is an
   error, but the text never does. It should say `id` must equal the
   manifest's identity.

8. **§5.2's manifest `version` has no grammar.** "Content version, dotted
   integers. Never `latest`." leaves open how many components are allowed,
   whether leading zeros are, and how it relates to §3.1's `vnum`. This
   implementation reuses `vnum` without the three-component cap, which is a
   guess no vector tests.

9. **§5.3's `integrity` gives no length or padding rule.** `sha256-<base64>`
   does not say the decoded value must be 32 bytes, or that padding is
   required. `manifests/invalid/bad-integrity.murl.json` (`sha256-short`) fails
   under either reading, so the vector does not disambiguate it: a lenient
   implementation that accepted `sha256-QUJD` would still pass the suite.

10. **`qchar` is used in the ABNF but never defined** (§3.1, `query =
    *512qchar`). The §3.2 charset rule bounds it to printable ASCII, and `#`
    has to be excluded or the selector cannot be split off, but the production
    itself is missing.

11. **"No control characters" is never given a code-point range.** The phrase
    appears in §3.2 (decoded segments) and §5.3 (targets, labels). C0 is
    obviously included; U+007F and the C1 block are not addressed. This
    implementation uses `< 0x20 || == 0x7F`, which is a guess — and note that
    is a *different* boundary from the one MCF-1 uses in (4), which is itself
    worth reconciling.

12. **§5.2's `name` and `description` limits count "chars", unqualified.**
    Everywhere else the spec's units are bytes, and the manifest size cap
    certainly is. For an ASCII vector the two readings agree; for a 120-emoji
    name they do not.

13. **§9 never says what to do with an unrecognized pre-1.0 `murlVersion`.**
    It gives the post-1.0 rule (reject a different major, accept a newer minor
    with warnings) and says 0.1 and 0.2 must be accepted, but
    `manifests/invalid/unsupported-version.murl.json` (`"9.9"`) is rejected
    only because the vector says so. Applying the post-1.0 rule to `9.9` gives
    the right answer here and the wrong one for a hypothetical `0.3`.

14. **A blank first line in `murls/invalid.txt` is a vector, or is not,
    depending on your harness.** `crates/murl-core/tests/conformance.rs`
    filters empty lines and sees 54; this harness keeps them and sees 55,
    because the empty string is itself an input a parser must reject. Both
    pass, and report different totals. The suite README should say which
    reading is intended — the count an implementer reports back should not
    depend on a detail nobody wrote down.

None of these blocked the implementation, and none required reading Rust
*logic*: items 1, 2, 7 and 14 were settled by conformance vectors, and 3, 4
and 5 by the module documentation in `canonical.rs`. For a spec at this stage
that is a good result. The gaps cluster in two places worth attention before
the format freezes: the canonical form, and the boundary between what
validation checks and what verification checks.
