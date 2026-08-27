# Security Model

An identifier that can make an operating system open many things is an
attack primitive by default. mURL's security model exists to make it a
*useful* primitive instead. This document states the model; the
[threat model](threat-model.md) enumerates the attacks it answers.

## Principles

1. **Manifests propose; local policy disposes.** A manifest describes what
   a destination is made of. It carries no permissions, cannot request
   permissions, and cannot configure handlers. Everything privileged —
   policy, trust pins, handler registrations, limits — lives in local,
   user-owned configuration that no manifest content can reach.

2. **Consent before dispatch, on the real plan.** The user is shown the
   fully resolved plan — every resource after splicing, its tier, the
   manifest's origin and trust status — before anything opens. Consent to a
   name is meaningless; consent is to the set.

3. **No shell, anywhere.** Every launch is an argv array passed to the
   process API. Target substitution happens inside a single argv element.
   There is no code path in which a target string is interpreted by
   `sh -c`, `cmd /c`, or any other shell.

4. **Fail closed.** Non-interactive invocation with no explicit allow flags
   dispatches nothing that needed consent. A hostile document that triggers
   the scheme handler in some headless context gets a refusal, not a
   best-effort.

5. **Hostile input stays behind small, fuzzed parsers.** The mURL parser,
   the manifest validator, and the canonicalizer are total functions with
   hard input caps, fuzzed continuously (`fuzz/`).

## Classification: three tiers

Classification derives from what a resource *is* — kind plus target — never
from what a manifest *says about itself*:

| Tier | What | Why |
|---|---|---|
| **SAFE** | `https` | rendered by a sandboxed browser; worst case ≈ a tab |
| **SENSITIVE** | `file`, `dir` (non-executable) | exposes/touches local data |
| **DANGEROUS** | `terminal`; `custom:*`; `file` whose extension is executable-adjacent (`.exe`, `.bat`, `.ps1`, `.sh`, `.desktop`, `.lnk`, `.jar`, `.hta`, …) | opening it is (or is one dialog from) code execution |

The executable-extension escalation closes a classic laundering trick:
"it's just a `file`" — where the file is a `.desktop` entry and `xdg-open`
would *execute* it. Classification looks at the target, not the label.

## Policy: what each tier requires

Defaults (all locally overridable in `config.json`, never by a manifest):

| Situation | Default outcome |
|---|---|
| SAFE resource | prompt (one consolidated consent step) |
| SENSITIVE resource | prompt |
| DANGEROUS resource, manifest trusted (LOCAL or pinned-key signed) | prompt |
| DANGEROUS resource, manifest untrusted | **deny — not promptable** |
| Remote manifest referencing local filesystem | extra consent reason (or deny, by policy) |
| Expired manifest | SAFE prompts with warning; others denied |

The untrusted-DANGEROUS rule is the keystone: users can be talked into one
click, and one click is exactly what a hostile mURL gets. Making terminals
from unknown senders *undeniable-by-click* removes the social-engineering
path entirely — the attacker's only routes are getting the user to run
`murl trust add` (a deliberate, documented act) or `murl name add` (ditto).

Consent flags are tier-scoped (`--yes` = SAFE only, `--allow-sensitive`,
`--allow-dangerous`) so scripts must state exactly how much risk they
accept.

## Resolution hardening

* **Grammar-level phishing defenses**: userinfo forbidden
  (`murl://github.com@evil…` cannot parse), ASCII-only identifiers, IDN
  authorities must be punycoded, dot segments rejected including encoded
  forms.
* **Network**: TLS required (loopback-only exception for http), zero
  redirects, size cap enforced *while reading*, per-fetch timeout, and DNS
  results filtered — non-loopback names resolving only to private /
  link-local / loopback / CGNAT ranges are refused (SSRF via
  `murl://printer.internal/…`).
* **Limits as security controls**: depth ≤ 3, ≤ 8 manifests, ≤ 64
  resources, ≤ 256 KiB per manifest, cycle detection on the resolution
  path, duplicate-target suppression. All breaches are hard errors; none
  are loosenable by manifest content.
* **Identity binding**: a manifest declaring `id` is refused if served
  under a different name — a valid signature cannot be replayed to relabel
  content (spec §6.4).
* **Integrity pins**: nested manifests can be pinned by `sha256-…` over
  their raw bytes; mismatch is a hard stop.
* **Cache integrity**: cached manifests are content-hashed; corrupt entries
  are dropped, not used.

## Dispatch hardening

* argv-only process creation; `{target}` substitution confined to a single
  element (a target containing `; rm -rf ~` remains one inert argument).
* File/dir/terminal targets must be absolute (or `~`-rooted), dot-segment
  free at validation time, and are existence-checked before launch.
* Sequential, staggered launching bounds the blast radius of even an
  approved 64-resource plan.
* `custom:*` kinds dispatch **only** through handlers the user registered
  out-of-band. An unregistered custom kind fails; it never guesses.

## What v0.1 does not defend against (known limitations)

Stated plainly, because a security model is defined by its edges:

* **A malicious *trusted* manifest.** If you pin a key or install a
  manifest locally, its DANGEROUS resources are one consent away. Trust is
  the boundary; choose pins accordingly.
* **What handlers do after launch.** Once the browser has the URL or the
  terminal is open, mURL's control ends. It does not sandbox handlers.
* **Duplicate-key JSON smuggling** is mitigated (one parser, last-wins,
  used identically for verification and interpretation) but duplicate keys
  are not yet rejected outright — planned hardening (roadmap).
* **Desktop-environment consent gaps.** On Linux, scheme activation opens a
  terminal for consent (`Terminal=true`); a compromised or terminal-less
  session degrades to fail-closed, not to a GUI prompt. The native consent
  surface is the v0.3 daemon's reason to exist.
* **Local attackers.** Config, trust store, and name store are plain files
  under the user's account, protected by OS file permissions only. An
  attacker who can write `~/.config/murl` has already won the account.

Report vulnerabilities per [SECURITY.md](../SECURITY.md).
