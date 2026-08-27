# Trust Model

## What a signature is, and is not

An mURL manifest can carry an ed25519 signature over its canonical bytes
(spec §7). A valid signature proves exactly one thing: **the holder of this
private key produced these bytes**. It does not prove the key holder is who
they claim, that the manifest is benign, or that you should run terminals
for them. Cryptographic validity and human trust are kept as separate
states on purpose:

```text
                          ┌───────────────────────────────────────────┐
                          │             manifest arrives              │
                          └───────────────┬───────────────────────────┘
              from local store /          │            fetched remotely
              explicit file               │
                     ▼                    ▼
               ┌──────────┐      signature present?
               │  LOCAL   │       no ──────────────▶ ┌────────────┐
               └──────────┘                          │  UNSIGNED  │
             (user-controlled;                       └────────────┘
              signature adds            yes
              nothing to its             │ verify (MCF-1 bytes)
              authority)                 ├─ invalid ──▶ HARD STOP
                                         ▼             (tamper evidence —
                              key pinned for this       resolution fails)
                              authority in the local
                              trust store?
                               no │        │ yes
                                  ▼        ▼
                        ┌────────────┐  ┌───────────┐
                        │   SIGNED   │  │  TRUSTED  │
                        │(unknown key)│ └───────────┘
                        └────────────┘
```

Policy consequence (the only place trust states matter in v0.1):
**DANGEROUS resources dispatch only from `LOCAL` or `TRUSTED` manifests.**
`SIGNED` is deliberately *not* trusted — anyone can sign anything with a key
they just generated.

## Pinning: trust is a local, per-authority decision

```bash
# Publisher side
murl keygen                        # ed25519 keypair, private key 0600
murl sign project-x.murl.json      # inserts the signature block
# serve at https://acme.example/.well-known/murl/project-x.murl.json

# Consumer side — a deliberate act, out of band of any mURL activation
murl trust add acme.example iPGnenpbuWg...   # or a key/manifest file
murl trust list
murl trust remove acme.example ed25519:3fe06954921ee77e
```

The trust store is one auditable JSON file mapping authority → pinned
public keys. Pinning is scoped: a key pinned for `acme.example` confers
nothing on `other.example`. Multiple keys per authority are allowed
(rotation: pin the new key, later remove the old).

Key identity is `ed25519:<first 16 hex of sha256(publicKey)>` — derived,
verifiable, and short enough for humans to compare.

## Why not PKI / web-of-trust / DID / transparency logs (in v0.1)

Each was considered; each was rejected for the same reason: **the mechanism
must be simple enough that its failure modes are enumerable.** X.509
chains, WoT graph evaluation, and DID method plurality all import large
attack surfaces and large explanations. Pinning-per-authority (TOFU-shaped,
but explicit rather than automatic) is small enough to audit by reading one
file and one function — and it composes forward: nothing prevents a later
version from *additionally* accepting, say, Sigstore bundles or a
transparency log for `@latest` rollback protection (threat T-16). Those are
listed in the roadmap as post-1.0 explorations.

## Companion mechanisms

* **Identity binding** (spec §6.4): the signed document names its own mURL
  in `id`; serving it under another name fails resolution. A signature
  cannot be replayed to relabel content.
* **Integrity pins**: a parent manifest may pin a nested manifest's exact
  bytes (`sha256-…`), giving composition-time immutability even across
  authorities.
* **Pinned versions** (`@1.4.2`): immutable by contract, cached forever —
  a version-pinned, integrity-pinned, signature-verified tree is fully
  reproducible.

## Operational guidance

* Treat `murl trust add` like adding an APT/YUM signing key: rare,
  deliberate, verified out-of-band (the publisher's website, not the mURL
  that's asking).
* Publishers: keep the private key offline from the web server; the
  well-known endpoint needs only the *signed* file.
* Losing a key: generate a new one, re-sign, publish the new public key;
  consumers re-pin. There is no revocation infrastructure in v0.1 —
  `expires` bounds how long a stolen key's signatures stay useful.
