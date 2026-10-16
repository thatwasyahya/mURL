# Privacy

mURL is a command-line tool that runs on your own computer. It has **no
telemetry, no analytics, no crash reporting, no advertising, no accounts and
no update checks**, and the project runs no server that the software
contacts. The mURL project collects nothing about you.

What follows is exactly what does leave your device, when, and to whom. It
describes mURL 0.5.1: the `murl` command and, where it is installed,
`murl-daemon`. Changes to this page are recorded in the repository history.

## What leaves your device

### Manifest downloads

mURL itself uses the network for one thing: downloading the manifest that
says what a remote mURL contains.

**When.** When you use a remote address (anything other than
`murl://local/…`) with a command that needs its manifest: `resolve`, `open`
(including `--dry-run`, and links you click after `murl os install`),
`validate`, `inspect`, `verify` or `export`. A running `murl-daemon`
downloads the same way when `murl open` hands it an address, or when another
program running as you asks it to.

If a fresh copy is already in the local cache, nothing is downloaded, unless
you pass `--refresh`. `resolve`, `open` and `export` also download every
remote mURL nested inside a manifest, each from its own host, and that
includes mURLs nested in a manifest stored on your computer. By default one
resolution reads at most 8 manifests, the address you gave included.

These downloads happen **before** mURL shows you what a destination
contains and asks for your consent. That is how it finds out what to show
you. `--offline` makes no request at all.

**Where to.** Only to the host named in each address:
`murl://example.com/team/onboarding` is fetched from
`https://example.com/.well-known/murl/team/onboarding.murl.json`. A nested
mURL can name any host, including `localhost`. Whoever runs that server, not
the mURL project, receives the request and decides what to do with it.

**What is sent.** A lookup of the host name through your system's resolver
(for a name such as `printer` or `printer.local`, your system may ask your
local network), then this HTTPS request:

```http
GET /.well-known/murl/team/onboarding.murl.json HTTP/1.1
Host: example.com
User-Agent: murl/0.5.1
Accept: application/murl+json, application/json
```

There are no cookies, no identifiers and no request body, and the request
carries nothing about your files or other resources. The `?query` and
`#selector` parts of the address are not sent. Like any web request, it
shows that server your IP address, the time, the address you asked for and
your mURL version. The host name also travels in the TLS handshake (SNI)
and in the lookup. While mURL is running it may resume a TLS session with a
server it already contacted, as browsers do, which lets that server link
those requests; `murl-daemon` keeps running until you stop it.

**Safeguards.**

* HTTPS only. Plain http is used just for `localhost`, `127.0.0.1` and
  `*.localhost`, and those requests can only reach your own computer.
* A host is refused when every address it resolves to is private,
  link-local or loopback; otherwise only its public addresses are used.
  Devices with public addresses are not covered by this check, which with
  IPv6 can include machines on your own network.
* Redirects are not followed, unless you turn them on with
  `limits.maxRedirects` in `config.json`. A redirect can then lead to
  another host, but an https request is never redirected to http.
* System proxy settings and proxy environment variables are not used.
* Certificates are checked against a built-in set of root certificates,
  with no revocation lookups, so no other server is contacted.

### What mURL hands to other programs

When you approve a resource, mURL passes its target to a program on your
computer. That program may then use the network on its own terms:

* web addresses, `mailto:` and `geo:` links, files and folders go to your
  default handler: `explorer.exe` on Windows, `xdg-open` on Linux, `open`
  on macOS, which hand each one to the app your system has set for it
  (browser, mail client, maps, file manager, or the app for that type of
  file);
* terminals, SSH, remote-desktop and `custom:` targets go only to a program
  you registered with `murl handler`.

mURL sends nothing about these resources itself. The one exception: after
you approve a file, folder or terminal, mURL checks that the path exists,
and on a network drive or a mounted share that check reaches the share's
server.

A manifest may not point a file, folder or terminal at a network path such
as `\\server\share` or `//server/share`: no target may start with two
slashes or backslashes. Paths inside shares you connected yourself are
still allowed, such as a mapped drive on Windows, or on Linux and macOS a
mounted share or an automounter path like `/net/…`.

### Clickable links

On Windows and Linux, `murl os install` makes `murl://` links clickable for
your user account. On Windows it writes `HKCU\Software\Classes\murl`; on
Linux, a desktop entry in `~/.local/share/applications` (or
`$XDG_DATA_HOME/applications`), which it sets as the default for `murl://`
links with `xdg-mime`. On macOS it registers nothing; links there need the
app bundle built from the source tree.

Clicking a link runs `murl open-url`, which downloads manifests as described
above before it shows the consent prompt. `murl os uninstall` deletes the
registry key or the desktop entry; on Linux, the `xdg-mime` default it set
stays behind but points at nothing. The winget, Scoop and Homebrew packages
never register links themselves.

## What stays on your device

| What | Windows | Linux | macOS |
|---|---|---|---|
| Your settings, handlers, pinned public keys and signing keys | `%APPDATA%\murl\` | `~/.config/murl/` | `~/Library/Application Support/murl/` |
| Names you install with `murl name` or `murl import` | `%LOCALAPPDATA%\murl\data\names\` | `~/.local/share/murl/names/` | `~/Library/Application Support/murl/data/names/` |
| Copies of downloaded manifests, each with the address it came from and when | `%LOCALAPPDATA%\murl\cache\` | `~/.cache/murl/` | `~/Library/Caches/murl/` |

`MURL_CONFIG_DIR`, `MURL_DATA_DIR` and `MURL_CACHE_DIR` override these
locations, and on Linux so do the `XDG_*` variables. `murl keygen --out`
saves a key wherever you choose instead. `murl-daemon`, which is not part of
the Windows release, uses the Linux locations on macOS as well and talks to
`murl` over a Unix socket that only your account can open.

mURL uploads none of this. On Windows, `%APPDATA%` is the roaming folder: if
your organization uses roaming profiles, Windows itself copies it, signing
keys included, to its servers. Signing keys from `murl keygen` are stored
unencrypted, so protect them as you would an SSH key. Diagnostics are
printed to the terminal; mURL writes no log file. Under the systemd user
service, `murl-daemon`'s output goes to your user journal, and that output
includes the address of every manifest it downloads.

To remove everything:

* `murl cache clear` empties the cache. On macOS it does not touch
  `murl-daemon`'s own cache in `~/.cache/murl/`.
* If you registered links, run `murl os uninstall` before uninstalling the
  package, which leaves the registration behind.
* If you used `murl-daemon`, stop it and run `murl-daemon service
  uninstall`.
* Then delete the `murl` folders above (on macOS, also `~/.config/murl/`,
  `~/.local/share/murl/` and `~/.cache/murl/` if you used `murl-daemon`) and
  any key you saved elsewhere with `murl keygen --out`.

## The website

<https://thatwasyahya.github.io/mURL/> is a static site hosted on GitHub
Pages. It has no analytics and no cookies, and it loads nothing from other
sites: the script libraries and fonts it uses are copies served from the
site itself. Your browser's local storage keeps your theme choice and which
tab each set of tabs last showed. The playground runs entirely in your
browser and sends nothing you type into it anywhere. GitHub, as the host,
processes request data under the
[GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Contact

Questions about this page: <https://github.com/thatwasyahya/mURL/issues>.
Security problems: see [SECURITY.md](SECURITY.md).
