# The mURL Daemon

`murl-daemon` exists for one reason: **consent deserves a better surface
than a terminal that may not exist.** Everything else it does (warm cache,
single-instance activation, an audit log) follows from being resident; none
of it would justify a resident process on its own.

```text
click murl://acme.example/project-x
        │
OS scheme association ──▶ murl open %u        (unchanged: the CLI is the handler)
        │
        ├── daemon reachable? ──▶ connect over the local socket
        │                          ├─ daemon resolves (its own core pipeline)
        │                          ├─ daemon presents consent (GUI dialog)
        │                          └─ daemon dispatches, streams outcomes back
        │
        └── no daemon? ─────────▶ resolve and prompt in-process, exactly as v0.2
```

The fallback is the whole design's safety net: **the daemon is an
optimization of the consent surface, never a dependency.** Kill it and mURL
still works, fail-closed, through the terminal path.

## IPC threat model

A local socket that can be told "open this destination" is a
privilege-adjacent primitive. It was threat-modeled before it was written
(roadmap v0.3 gate); these are the entries that extend
[docs/threat-model.md](threat-model.md).

### D-1 · Any local process asks the daemon to open something

Any process running as the user could connect and submit an mURL.

**Defenses**: the socket is a **user-private** endpoint —
`$XDG_RUNTIME_DIR/murl/murl.sock` with mode `0600` inside a `0700`
directory (Unix), or a named pipe with a user-only DACL (Windows). More
importantly, submitting an mURL grants nothing: every request runs the same
resolve → classify → **consent** pipeline. A hostile local process can at
most cause a dialog to appear — which it could already do by running
`murl open` directly. The daemon deliberately exposes **no** endpoint that
dispatches without consent.

### D-2 · Consent-dialog spoofing / clickjacking (partial)

A malicious process paints a fake mURL consent dialog, or races a real one
to steal the click.

**Defenses**: the dialog always shows the *resolved* facts — authority,
origin, trust status, per-resource tier — rather than a caller-supplied
description; DANGEROUS resources from untrusted manifests never appear as
approvable (they are denied before the dialog). **Residual**: X11 offers no
input-integrity guarantee; a compositor-level portal (Wayland) is the
correct long-term answer and is why the dialog layer is abstracted behind
`ConsentUi` rather than hard-wired.

### D-3 · Request flooding / resource exhaustion

A local process submits thousands of activations.

**Defenses**: one in-flight activation per connection; a global cap on
concurrent connections; a per-request byte cap and read timeout; the
existing resolution limits (depth/count/size) apply unchanged; dispatch
stays sequential and staggered. Excess connections are refused, not queued
unboundedly.

### D-4 · Protocol confusion / parser attacks

Malformed frames, oversized payloads, embedded newlines, non-UTF-8.

**Defenses**: the wire format is newline-delimited JSON with a hard
per-line byte cap, parsed by the same strict (duplicate-rejecting) parser
as manifests. Requests are a closed enum; unknown request types are
refused, never "best-effort" interpreted. The daemon never evaluates code,
never shells out, and passes targets to the same argv-only launcher.

### D-5 · Socket squatting / stale socket hijack

An attacker pre-creates the socket path to intercept requests, or a crashed
daemon leaves a stale socket that a later attacker replaces.

**Defenses**: the daemon creates its socket inside a directory it verifies
is owned by the user and mode `0700`; on bind failure it probes the
existing socket and refuses to clobber a live one. Clients verify the
socket's ownership before connecting and fall back to in-process
resolution if anything is off, rather than trusting an unknown listener.

### D-6 · Privilege escalation via the daemon

**Defenses**: the daemon runs as the user, never as root, never setuid, and
holds no capability the user lacks. It is explicitly *not* a system
service; there is no system-wide unit, and installing one is documented as
unsupported.

### D-7 · State leakage through the audit log (partial)

The log records which destinations were opened and when.

**Defenses**: the log lives under the user's data directory with `0600`
permissions, records identities and outcomes but never manifest bodies or
credentials, and is opt-in via configuration. **Residual**: it is still a
record of activity on disk; users who don't want one leave it off (the
default).

## Wire protocol (v0.3, experimental)

Newline-delimited JSON, one request per line, one or more responses per
request. Deliberately boring — a protocol you can debug with `socat`.

```text
→ {"type":"ping","protocol":1}
← {"type":"pong","protocol":1,"version":"0.3.0"}

→ {"type":"resolve","murl":"murl://local/project-x"}
← {"type":"plan","resolution":{ …the same JSON as `murl resolve --json`… }}

→ {"type":"activate","murl":"murl://local/project-x"}
← {"type":"plan","resolution":{…}}          # what will be asked about
← {"type":"consent","granted":["docs"],"denied":["term"]}
← {"type":"outcome","report":{…}}           # per-resource + aggregate

← {"type":"error","stage":"resolve","message":"…"}
```

Rules: `protocol` is an integer the daemon must match exactly (no
negotiation, no downgrade); every request is independent; the daemon never
initiates. Consent decisions are made **inside** the daemon (by its UI), so
a client cannot pre-approve anything on the user's behalf — a client asking
to activate is asking for a dialog, not for a launch.

## The consent surface

At startup the daemon picks the best surface available and says which one it
chose:

```text
murl-daemon: consent surface: zenity (/usr/bin/zenity)
```

The order is: a **native dialog** (`zenity`, `kdialog`, or `osascript` on
macOS, requiring a display), then the **terminal**, then **denial**. The
chain only ever gets stricter — a missing surface can never become a
permissive one.

The dialog is built on the helper each desktop already ships rather than on
a toolkit dependency, and it keeps three properties:

* **No shell and no generated script.** Backends are invoked as argv
  arrays. For `osascript` the AppleScript source is a *constant* and the
  plan travels in `argv`; interpolating a target into script text would be
  the same mistake as building a shell command, one language over.
* **The dialog returns resource ids and nothing else.** Ids are
  `[a-z0-9][a-z0-9_-]*`, so a returned line cannot be confused with a
  separator or another resource's text — and anything returned that was
  not offered is discarded. A backend cannot grant what policy denied.
* **Every failure is a denial**: no backend, a crash, a closed window, an
  unanswered prompt (180 s), or unparseable output.

Nothing is pre-checked. Consent starts from no.

## Status

The daemon is **experimental** and ships as its own binary. The CLI's
`--daemon`/`--no-daemon` flags select the path explicitly; the default is to
try the daemon and fall back silently. The `ConsentUi` abstraction is what
made the dialog a drop-in: neither the protocol nor the security model
changed when the pixels arrived.
