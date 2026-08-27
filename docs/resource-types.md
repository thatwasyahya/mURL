# Resource Types

The kind registry of mURL v0.1. Kinds determine target validation
(spec §5.3), risk tier (docs/security.md), and dispatch.

## Built-in kinds

### `https` — web resources · SAFE

Target: an `https://` URL (plain `http://` for loopback hosts only; no
userinfo; no spaces). Dispatch: platform opener → default browser
(`xdg-open` / `open` / `explorer.exe`). This covers every web application:
repos, docs, trackers, dashboards, chat rooms.

### `file` — a local file · SENSITIVE (or DANGEROUS)

Target: absolute or `~`-rooted path, no dot segments. Dispatch: platform
opener → the file's associated application, after an existence check
(missing ⇒ `UNAVAILABLE`, not a launch error). **Escalation**: an
executable-adjacent extension (`.exe`, `.bat`, `.cmd`, `.ps1`, `.sh`,
`.desktop`, `.lnk`, `.jar`, `.hta`, `.msi`, `.appimage`, …) reclassifies the
resource as DANGEROUS, because "opening" such a file executes it.

### `dir` — a local directory · SENSITIVE

Target: as `file`. Dispatch: platform opener → file manager.

### `murl` — a nested destination · container

Target: a valid mURL. Never dispatched itself; its resolution is spliced
into the plan under the recursion limits (docs/resolution.md). Supports
`integrity` pinning of the child manifest's exact bytes. This is the
composition primitive: `team` inside `project-x`, `org` inside `team`.

### `ssh` — a remote shell · DANGEROUS

Target: `ssh://[user@]host[:port]`. Dispatch requires a configured handler:

```bash
murl handler set-ssh -- x-terminal-emulator -e ssh {target}
```

Userinfo is permitted here and **nowhere else** in mURL: an ssh target
without a username is often unusable, and unlike a web URL there is no
address-bar confusion to inherit — the target the plan shows is the target
that connects, and connecting is DANGEROUS-tier regardless. The validator
still refuses option smuggling (`ssh://-oProxyCommand=…@host`), a second
`@`, and any character outside `[A-Za-z0-9._-]` in the username or
`[A-Za-z0-9.-]` in the host.

### `remote-desktop` — a remote GUI session · DANGEROUS

Target: `rdp://host[:port]` or `vnc://host[:port]`. Userinfo is *not*
allowed (these clients take credentials interactively). Handler:

```bash
murl handler set-remote-desktop -- xfreerdp {target}
```

### `geo` — a map location · SAFE

Target: `geo:lat,lon[,alt][;u=radius]` (RFC 5870), range-checked
(−90..90, −180..180). Dispatched to the platform opener → map viewer. SAFE
because it conveys no capability: worst case, a map opens somewhere
uninteresting.

### `mailto` — a pre-addressed draft · SAFE

Target: `mailto:addr[,addr][?subject=…&body=…&cc=…&to=…]` (RFC 6068).
Headers are restricted to that safe list — a manifest may pre-fill a
subject or body, but must not add a `bcc` the user won't notice, and no
header a client might act on beyond composing. SAFE because composing is
not sending: every mail client shows the draft first.

### `terminal` — a shell session · DANGEROUS

Target: a directory path (the working directory). Dispatch requires a
locally configured handler:

```bash
murl handler set-terminal gnome-terminal --working-directory={target}
murl handler set-terminal wezterm start --cwd {target}
```

Unset ⇒ terminal resources fail with guidance; they never guess a terminal
emulator. Always DANGEROUS: a terminal is arbitrary code execution by
definition, so untrusted manifests can't reach it at all (trust gate) and
trusted ones still prompt.

## Extension kinds: `custom:<name>` · DANGEROUS

The open extension point. A manifest may declare `custom:vscode`,
`custom:zoom`, `custom:psql`… — and nothing happens unless the *user* has
registered a handler for that name:

```bash
murl handler register vscode -- code --folder-uri {target}
murl handler list
murl handler remove vscode
```

Dispatch substitutes `{target}` inside single argv elements (never a
shell); with no `{target}` in the template, the target is appended as the
final argument. Registration is deliberately asymmetric: manifests name
capabilities, only local configuration maps names to programs. Custom kinds
are uniformly DANGEROUS because the resolver cannot know what a registered
program does with a target.

**Choosing between `https` and `custom`**: if the tool has a web URL, use
`https` — it is SAFE-tier and universally handled. Reach for `custom:` only
when a native program with its own argument shape is genuinely required.

## The `role` vocabulary (orthogonal to kind)

`role` says what a resource *means* in the destination; `kind` says what it
*is*. Conventional roles: `source`, `docs`, `issues`, `monitoring`,
`workspace`, `terminal`, `chat`, `api`, `design`, `ci`. The vocabulary is
open (validated shape, not membership); tooling may group or filter by
role, e.g. a future `murl open …#role=docs`.

## Adding a built-in kind (spec change)

1. Grammar/validation: `kind.rs` (`Kind::parse`), `manifest.rs`
   (`validate_target`).
2. Classification: `policy.rs` (`classify`) — justify the tier in
   docs/security.md.
3. Dispatch: `dispatch.rs` (`dispatch_one`) + any `OpenerConfig` needs.
4. Spec §5.3 table + this document + tests (validator, classifier,
   dispatch, and a hostile-target case in the security suites).

Candidates under consideration are listed in docs/roadmap.md (`ssh`,
`vnc`/`rdp`, `mailto`-equivalent, `geo`).
