# IANA Media Type Registration Template — `application/murl+json`

**Status: DRAFT, not submitted.** Template per
[RFC 6838](https://www.rfc-editor.org/rfc/rfc6838) (media type
registration) and [RFC 6839](https://www.rfc-editor.org/rfc/rfc6839)
(`+json` structured syntax suffix), for registration in the **vendor** or
**standards** tree once the format stabilizes. Kept in-repo so the path is
concrete; submission is gated on the same v1.0 criteria as the scheme
registration.

---

**Type name:** application

**Subtype name:** murl+json

**Required parameters:** none

**Optional parameters:** none

**Encoding considerations:** binary (UTF-8 JSON, per RFC 8259)

## Security considerations

An mURL manifest describes a set of resources that software may open on a
user's behalf, including local files and process-starting resources. It is
therefore **untrusted input in the strongest sense** and consumers must
treat it as such:

* Enforce a byte limit *before* parsing (262 144 bytes recommended).
* Reject duplicate object members at every nesting level. Duplicates are a
  cross-implementation signature-confusion vector: two conformant
  consumers could verify one signature yet act on different values.
* Require all numbers to be integers; non-integer numbers fall outside the
  canonical form and make a document unsignable.
* Validate every field before use — resource identifiers, per-kind target
  syntax, referential integrity of dependency edges, and acyclicity.
* Never dispatch a resource without classifying it and obtaining user
  consent; never grant privileges based on manifest content.
* Manifests may carry an ed25519 signature over a canonical form (see the
  specification). A present-but-invalid signature must be a hard failure,
  not a downgrade to "unsigned". A valid signature attests authorship
  only; trust is a separate, local decision.
* A manifest may declare its own identifier (`id`); consumers should
  refuse a manifest served under a different name, which prevents replaying
  a valid signature under an unintended identity.

Full analysis: `docs/threat-model.md` in the reference repository.

## Interoperability considerations

The format is JSON with a documented schema
(`spec/murl-manifest.schema.json`, descriptive) and a normative
specification (`spec/SPECIFICATION.md`). Unknown members must be ignored
for forward compatibility and are covered by signatures. A conformance
vector suite (`spec/conformance/`) exists for implementers.

**Published specification:** `spec/SPECIFICATION.md`, versioned with the
reference implementation at https://github.com/thatwasyahya/mURL

**Applications that use this media type:** mURL resolvers, including the
reference CLI and daemon.

**Fragment identifier considerations:** The `+json` suffix implies the
fragment rules of `application/json` (RFC 6839 §3.1); the format defines
no fragment semantics of its own. Note that `#selector` in a `murl:` URI
addresses the *resolved resource set*, not a location inside the manifest
document.

**Additional information:**

* Deprecated alias names: none
* Magic numbers: none
* File extension: `.murl.json`
* Macintosh file type code: `TEXT`
* Object Identifiers: none

**Person & email address to contact for further information:** mURL
maintainers, via the repository above.

**Intended usage:** COMMON

**Restrictions on usage:** none

**Author / Change controller:** The mURL project maintainers (provisional).

**Provisional registration:** yes (pending specification stabilization).
