# Security Policy

mURL's entire purpose is to make "one link opens many things" safe; security
reports are first-class contributions here.

## Reporting a vulnerability

**Please do not open public issues for vulnerabilities.**

Use GitHub's **private vulnerability reporting** ("Report a vulnerability"
under the Security tab of this repository). Include: the affected component
(parser / validator / resolver / policy / trust / dispatch / CLI), a
reproduction (a hostile mURL or manifest is usually the whole PoC), and the
impact you believe it has against the model in
[docs/threat-model.md](docs/threat-model.md).

You can expect an acknowledgment within **7 days** and a triage verdict
within **14**. Coordinated disclosure is appreciated; we'll agree on a
timeline with you (default 90 days). Credit is given in release notes
unless you ask otherwise.

## Scope

In scope, with priority:

* Anything that dispatches a resource **without the consent/policy path**
  (fail-open behavior).
* Shell or argument injection through any target, label, or config value.
* Parser/validator crashes or differentials (the fuzz targets missing
  something).
* Signature/trust bypasses: forged verification, identity-binding bypass,
  trust-store confusion, integrity-pin bypass.
* Limit bypasses (recursion, size, count) and SSRF-filter bypasses.

Out of scope: behavior of third-party handler applications after launch;
attacks requiring write access to the victim's user account (see
threat-model "out of scope"); the `examples/` content.

## Supported versions

Pre-1.0, only the latest release (and `main`) receive fixes.

## Handling expectations

Fixes for confirmed vulnerabilities land with regression tests and, where
the class allows it, a new fuzz-corpus entry so the class stays dead.
