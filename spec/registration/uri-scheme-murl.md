# IANA URI Scheme Registration Template — `murl`

**Status: DRAFT, not submitted.** This is the template that would accompany
a *provisional* registration request per
[RFC 7595](https://www.rfc-editor.org/rfc/rfc7595) §5.2 (Expert Review,
`uri-review@ietf.org` discussion, then IANA). It is kept in-repo so the
registration path is concrete and reviewable rather than aspirational.

Submitting is gated on the roadmap's v1.0 criteria: a second independent
implementation passing `spec/conformance/`, and real deployment feedback.
Registering a scheme is a claim on a global namespace; making it before the
format has been exercised by someone else would be the wrong order.

---

**Scheme name:** `murl`

**Status:** Provisional

**Applications/protocols that use this scheme name:**
The mURL (Multi-Resource Uniform Locator) resolver family. An mURL names a
*logical destination* — a set of heterogeneous resources (web resources,
local files and directories, terminal or remote sessions, nested mURLs) —
rather than a single resource. Operating-system URL handlers pass `murl:`
URIs to a resolver, which retrieves a manifest describing the set, applies
a local security policy, obtains user consent, and dispatches each resource
to its usual handler. Reference implementation:
https://github.com/thatwasyahya/mURL

**Contact:** mURL maintainers, via the repository above.

**Change controller:** The mURL project maintainers (provisional). If the
specification advances, change control would move to the standards body
that adopts it.

**References:** `spec/SPECIFICATION.md` in the repository above (versioned
with the implementation; v0.2 at the time of writing).

## Scheme syntax

```abnf
murl        = "murl://" authority "/" name [ "@" version ]
              [ "?" query ] [ "#" selector ]
authority   = "local" / host [ ":" port ]
name        = segment *( "/" segment )        ; 1..8 segments, <=64 bytes each
version     = "latest" / vnum *2( "." vnum )
selector    = sel-item *7( "," sel-item )
sel-item    = resource-id / "role=" role / "tag=" tag
```

The syntax is a restriction of RFC 3986 generic syntax. Deliberate
restrictions, all security-motivated:

* **Userinfo is forbidden** in the authority. `murl://example.com@evil.example/x`
  is a parse error, not a lookup against `evil.example`.
* The whole URI is **printable ASCII**; non-ASCII must be percent-encoded
  UTF-8, and internationalized authorities must be punycoded.
* **Dot segments** (`.`, `..`), including percent-encoded forms, are
  rejected.
* Total length is capped (1024 bytes in the reference implementation).

## Scheme semantics

A `murl:` URI identifies a *destination*: a named, versionable set of
resources described by a manifest (media type `application/murl+json`,
see the companion template). The URI itself never contains resources.

* `local` is a reserved authority whose names resolve against a
  user-controlled local store and never cause network access.
* Any other authority is a DNS name; the manifest is retrieved over HTTPS
  from that authority's well-known location:
  `https://<authority>/.well-known/murl/<name>[@<version>].murl.json`
  (see the companion `.well-known` registration note below).
* `@version` selects an immutable published version; its absence means
  `latest`, a mutable alias.
* `#selector` narrows the destination to a subset of its resources
  (by resource id, `role=`, or `tag=`). It is *not* a fragment into a
  retrieved representation in the classical sense; it filters the resolved
  set. Implementations must treat a selector that matches nothing as an
  error.

## Encoding considerations

Name segments are percent-encoded UTF-8; the canonical form leaves
unreserved characters (`A-Z a-z 0-9 - . _ ~`) literal and encodes all
others as uppercase `%XX`. Authorities are lowercased. Percent-encoded
path separators and dot segments are invalid, not merely discouraged.

## Interoperability considerations

`murl:` URIs are inert to software that does not implement the scheme:
they neither resemble nor rewrite to other schemes. Every *leaf* resource
in a manifest is addressed by an existing scheme (`https:`, `file:`,
`ssh:`, `geo:`, `mailto:`), so mURL composes with the existing ecosystem
rather than replacing any part of it.

## Security considerations

Activating a `murl:` URI can cause an operating system to open multiple
resources, some of which touch local data or start processes. The
specification therefore places security in the resolution pipeline itself,
not in applications:

* Resources are classified SAFE / SENSITIVE / DANGEROUS from what they
  *are* (kind and target), never from what the manifest claims.
* Manifests carry **no permissions**; local policy decides. User consent
  is obtained against the fully resolved plan before anything opens.
* DANGEROUS resources additionally require trust (a locally installed
  manifest, or one signed by a key the user pinned for that authority) —
  they are refused, not merely prompted, from untrusted sources.
* Dispatch constructs argv arrays; no target is ever passed through a
  shell.
* Resolution enforces hard limits (nesting depth, resource and manifest
  counts, byte sizes, timeouts), rejects redirects, refuses hosts that
  resolve only to private/link-local ranges (SSRF), and detects cycles.
* Manifests may be signed (ed25519 over a canonical JSON form) and bound
  to their own identity, so a signature cannot be replayed under another
  name.

Full analysis: `docs/threat-model.md` and `docs/security.md` in the
repository.

## Well-known URI note

The manifest location uses a `/.well-known/murl/` prefix
([RFC 8615](https://www.rfc-editor.org/rfc/rfc8615)). A separate
registration of the `murl` well-known suffix would accompany the scheme
registration; the template is kept with this one when submission
approaches.
