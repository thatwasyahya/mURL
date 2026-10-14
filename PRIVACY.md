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
`validate`, `inspect`, `verify` or `export`. If a fresh copy is already in
the local cache, nothing is downloaded. Every mURL nested inside a manifest
is downloaded the same way from its own host, up to 8 per resolution by
default, and that includes mURLs nested in a manifest stored on your
computer.

These downloads happen **before** mURL shows you what a destination
contains and asks for your consent. That is how it finds out what to show
you. `--offline` makes no request at all.

**Where to.** Only to the host named in the address:
`murl://example.com/team/onboarding` is fetched from
`https://example.com/.well-known/murl/team/onboarding.murl.json`. Whoever
runs that server, not the mURL project, receives the request and handles it
under their own privacy policy.

**What is sent.** A DNS lookup of the host name through your system's
resolver, then this HTTPS request:

```http
GET /.well-known/murl/team/onboarding.murl.json HTTP/1.1
Host: example.com
User-Agent: murl/0.5.1
Accept: application/murl+json, application/json
```

There are no cookies, no identifiers, no request body and nothing about your
files or other resources. The `#part` and any `?…` query in the address are
not sent. Like any web request, it shows that server your IP address, the
time, the address you asked for and your mURL version. The host name also
travels in the TLS handshake (SNI) and in the DNS lookup.

**Safeguards.**

* HTTPS only. Plain http is used just for `localhost`, `127.0.0.1` and
  `*.localhost`, and those requests can only reach your own computer.
* Hosts that resolve to private, link-local or loopback addresses are
  refused, so a link cannot make mURL probe your local network.
* Redirects are not followed, unless you turn them on with
  `limits.maxRedirects` in `config.json`. Even then, an https request is
  never redirected to http.
* System proxy settings and proxy environment variables are not used.
* Certificates are checked against a built-in set of root certificates,
  with no revocation lookups, so no other server is contacted.

### What mURL hands to other programs

When you approve a resource, mURL passes its target to a program on your
computer. That program may then use the network on its own terms:

* web addresses, `mailto:` and `geo:` links, files and folders go to your
  default handler: `explorer.exe` on Windows (which hands them to your
  browser, mail client or file manager), `xdg-open` on Linux, `open` on
  macOS;
* terminals, SSH and remote-desktop targets go only to a program you
  registered with `murl handler`.

mURL sends nothing about these resources itself. A manifest may not point a
file, folder or terminal at a network share (`\\server\share` or
`//server/share`). A path on a network drive you mapped yourself is reached
by Windows, as it would be for any other program.

### Clickable links

`murl os install` makes `murl://` links clickable for your user account. On
Windows it writes `HKCU\Software\Classes\murl`; on Linux, a desktop entry in
`~/.local/share/applications`. Clicking a link then runs `murl open-url`,
which downloads manifests as described above before it shows the consent
prompt. `murl os uninstall` removes the registration. The winget, Scoop and
Homebrew packages never register links themselves.

## What stays on your device

| What | Windows | Linux | macOS |
|---|---|---|---|
| Your settings, handlers, pinned public keys and signing keys | `%APPDATA%\murl\` | `~/.config/murl/` | `~/Library/Application Support/murl/` |
| Names you install with `murl name` or `murl import` | `%LOCALAPPDATA%\murl\data\names\` | `~/.local/share/murl/names/` | `~/Library/Application Support/murl/data/names/` |
| Copies of downloaded manifests | `%LOCALAPPDATA%\murl\cache\` | `~/.cache/murl/` | `~/Library/Caches/murl/` |

`MURL_CONFIG_DIR`, `MURL_DATA_DIR` and `MURL_CACHE_DIR` override these
locations, and on Linux so do the `XDG_*` variables. `murl-daemon`, which is
not part of the Windows release, uses the Linux locations on macOS as well
and talks to `murl` over a Unix socket that only your account can open.

None of this is uploaded anywhere. Signing keys from `murl keygen` are
stored unencrypted, so protect them as you would an SSH key. Diagnostics are
printed to the terminal; mURL writes no log file. Under the systemd user
service, `murl-daemon`'s output goes to your user journal.

To remove everything: run `murl cache clear` if you only want the cache
gone. For all of it, run `murl os uninstall` first if you registered links
(uninstalling the package leaves the registration behind), then delete the
folders above.

## The website

<https://thatwasyahya.github.io/mURL/> is a static site hosted on GitHub
Pages. It has no analytics, no cookies and no third-party scripts or fonts.
It remembers your theme and tab choices in your browser's local storage. The
playground runs entirely in your browser, and nothing typed into it is sent
anywhere. GitHub, as the host, processes request data under the
[GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Contact

Questions about this page: <https://github.com/thatwasyahya/mURL/issues>.
Security problems: see [SECURITY.md](SECURITY.md).
