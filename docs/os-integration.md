# OS Integration

How `murl://…` becomes clickable. The adapter surface is deliberately thin:
register a scheme handler that runs `murl open %u`, and provide the platform
opener argv. Everything else (consent, policy, dispatch) is platform-neutral
core.

```text
click murl://acme.example/project-x
        │  (chat app, browser, mail client, qr scanner…)
        ▼
OS scheme association (x-scheme-handler/murl · HKCU Classes · LS)
        ▼
murl open "murl://acme.example/project-x"
        ▼
resolve → plan → consent → dispatch (see docs/resolution.md)
```

## Linux — implemented

`murl os install` writes
`~/.local/share/applications/murl-handler.desktop`:

```ini
[Desktop Entry]
Type=Application
Name=mURL Resolver
Exec="/path/to/murl" open %u
Terminal=true
NoDisplay=true
MimeType=x-scheme-handler/murl;
```

then runs `xdg-mime default murl-handler.desktop x-scheme-handler/murl` and
refreshes the desktop database (best-effort). `murl os status` queries the
association; `murl os uninstall` removes the entry. Works across
XDG-compliant environments (GNOME, KDE, etc.); browsers route
`murl://` links through the same association.

`Terminal=true` is a deliberate v0.1 compromise: consent needs an
interactive surface, and the CLI's surface is a TTY. Environments that
ignore `Terminal=true` degrade to fail-closed (no TTY ⇒ nothing needing
consent opens) — never to silent approval. The native consent dialog is the
core deliverable of the v0.3 daemon (docs/roadmap.md).

D-Bus was evaluated and not used in v0.1: with no daemon there is no
service to talk to, and desktop-file activation already covers launching.
It becomes relevant together with the daemon (single-instance activation,
portal-style consent).

## Windows — implemented (per-user)

`murl os install` writes, via `reg.exe` with argv arrays (never a shell):

```text
HKCU\Software\Classes\murl
    (Default)      = "URL:mURL Protocol"
    URL Protocol   = ""
    shell\open\command\(Default) = ""C:\path\murl.exe" open "%1""
```

Per-user (HKCU), so no elevation; `murl os status`/`uninstall` query and
delete the same key. The `%1` arrives as a single argv element to
`murl.exe` — Windows performs no shell expansion on it, and the mURL parser
re-validates it from scratch anyway.

Console note: activation opens a console window for consent, the same
compromise as Linux's `Terminal=true`, with the same fail-closed fallback.

## macOS — app bundle

Launch Services reads URL scheme claims (`CFBundleURLTypes`) from an
application bundle's `Info.plist` at registration time; **a bare CLI binary
cannot claim a scheme**, whatever it writes to disk. So mURL ships a
bundle:

```bash
packaging/macos/build-app.sh --release      # produces target/macos/mURL.app
open target/macos/mURL.app                  # registers with Launch Services
# or: lsregister -f target/macos/mURL.app
open 'murl://local/project-x'
```

The bundle is scaffolding, not a second implementation: `Info.plist`
declares the `murl` scheme, and `Contents/MacOS/mURL` is a three-line stub
that `exec`s the bundled `murl open "$@"`. Launch Services hands the
activated URL as `$1`; the stub passes it through unchanged, because the
parser re-validates it from scratch and nothing in a shell wrapper should
be interpreting attacker-supplied input. Every security property comes from
the binary it wraps.

`murl os install` on macOS still explains the bundle requirement and exits
non-zero rather than pretending it registered something — the bundle script
is the supported path. Dispatch itself has always worked on macOS via
`open` (`OpenerConfig::platform_default("macos")`).

**Known limitation — macOS activation currently denies everything.** A
Launch Services activation runs the bundle's executable with no controlling
terminal, and consent without a way to ask is a refusal (that is the
fail-closed rule, working as designed). Linux sidesteps this with
`Terminal=true` and Windows with a console window; macOS bundles have no
equivalent, so `open 'murl://local/x'` resolves, prints the plan, and exits
`4` (DENIED) rather than opening anything.

That makes macOS registration a **preview**: correct and safe, but not yet
useful for anything that needs consent. Two ways out, both on the roadmap
and neither faked here — the native consent dialog (v0.3's remaining work,
which is exactly what the `ConsentUi` abstraction exists for), or a policy
of `"safe": "allow"` for users who accept that trade for SAFE resources.
Wrapping the launcher in a Terminal.app invocation was considered and
rejected: it means writing the activated URL into a script for another
program to re-interpret, which is precisely the shell-shaped surface this
project refuses to have.

Not yet done either: notarized/stapled release artifacts, which need an
Apple developer identity (roadmap v0.4). Until then, Gatekeeper will warn on
a downloaded bundle; building locally avoids that entirely.

## Security notes common to all platforms

* Registration is per-user and reversible; `murl os install` touches
  nothing system-wide and never elevates.
* The handler command is the resolver, not a browser or shell — every
  activation goes through the full parse/validate/policy pipeline; there is
  no "fast path" for OS-delivered input.
* **Every platform registers `murl open-url`, not `murl open`.** The two
  differ in one way that matters: `open-url` ignores approval flags even if
  they appear in argv. This is a defense against how Windows delivers the
  URL — ShellExecute substitutes it into a command *template* string, and
  `CommandLineToArgvW` parses afterwards, so a link containing a quote can
  append arguments (`murl://local/x" --allow-dangerous`). With approval
  flags inert on the handler path, the most such an injection achieves is a
  prompt the user still has to answer. Linux (`%u` substituted as one argv
  element after the `Exec` line is parsed) and macOS were never exposed the
  same way, but they use the same entry point so there is one rule to
  reason about rather than three.
* An attacker-supplied *identifier* is the expected case, not a special
  one: the parser's job description is hostile input (threat model T-1).
* `murl os install` uses the current executable's absolute path; moving the
  binary requires re-running install (status will show the stale path).
