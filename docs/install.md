# Installing mURL

> mURL is **experimental** (format v0.2, release v0.5.0). Installing it gets
> you a working reference implementation of a proposed primitive — not a
> standard, and not something with a compatibility promise yet. See
> [stability.md](stability.md) for which surfaces carry which label.

Two binaries exist:

| Binary | What it is | Needed? |
|---|---|---|
| `murl` | the CLI: parse, validate, resolve, sign, open, register the scheme | yes |
| `murl-daemon` | a resident resolver providing a persistent consent surface and warm cache ([daemon.md](daemon.md)) | no — the CLI falls back to in-process resolution |

Nothing about installing registers the `murl://` scheme or starts anything.
That is a separate, explicit step: see [making murl:// clickable](#making-murl-clickable)
below.

## What actually works today

Being blunt about it, because a page full of install commands that 404 is
worse than a short one:

| Channel | Status |
|---|---|
| `cargo install` from git | **works** |
| build from source | **works** |
| prebuilt release archives | **works** for three targets (see below) |
| Homebrew | **published** — `brew tap thatwasyahya/murl && brew install murl` ([tap](https://github.com/thatwasyahya/homebrew-murl)) |
| crates.io | **published** — `cargo install murl-cli` (and `murl-daemon`) |
| Scoop · Nix · winget | **manifests written, not yet published** — [packaging/](../packaging/) has them and what remains to be done |
| AUR | **blocked upstream** — new AUR account registration is paused while Arch deals with automated signups; the PKGBUILD is ready in [packaging/aur/](../packaging/aur/) |

## cargo install from git

The shortest path if you have a Rust toolchain (1.75 or newer):

```bash
cargo install --git https://github.com/thatwasyahya/mURL --tag v0.5.0 murl-cli

# optional, and only if you want the resident consent surface:
cargo install --git https://github.com/thatwasyahya/mURL --tag v0.5.0 murl-daemon
```

The crate name is `murl-cli`; the binary it installs is `murl`. Both land in
`~/.cargo/bin`, which needs to be on your `PATH`.

Pin the tag. `--branch main` tracks a moving target, and in a pre-1.0 project
that is a genuine risk rather than a theoretical one — minor versions may
break anything.

mURL is not on crates.io yet, so there is no plain `cargo install murl`.

## Build from source

```bash
git clone https://github.com/thatwasyahya/mURL && cd mURL
git checkout v0.5.0
cargo build --release
```

Binaries appear at `target/release/murl` and `target/release/murl-daemon`.
Copy them somewhere on your `PATH`, or run them in place.

Dependencies are pure Rust — TLS is rustls, so there is no OpenSSL and no
system library to install beyond a C toolchain for linking.

Worth doing once before you trust it with anything:

```bash
cargo test              # 120+ tests, including the security suites
bash examples/demo.sh   # hermetic guided tour: temp state, dry-run, cleanup
```

## Prebuilt archives

Each tagged release publishes archives on the
[releases page](https://github.com/thatwasyahya/mURL/releases), for three
targets:

| Platform | Archive |
|---|---|
| Linux x86_64 | `murl-v0.5.0-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple silicon | `murl-v0.5.0-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `murl-v0.5.0-x86_64-pc-windows-msvc.zip` |

Intel macOS, Arm64 Windows and Arm64 Linux are **not** built. Use `cargo
install` or a source build there.

The archives contain `murl` only — not `murl-daemon`. If you want the daemon,
build it.

```bash
tar xzf murl-v0.5.0-x86_64-unknown-linux-gnu.tar.gz
install -m0755 murl-v0.5.0-x86_64-unknown-linux-gnu/murl ~/.local/bin/murl
```

### Verifying a download

Every archive has a `.sha256` file beside it. On Linux and macOS the file is
in `shasum` format, so it can be checked directly:

```bash
base=https://github.com/thatwasyahya/mURL/releases/download/v0.5.0
name=murl-v0.5.0-x86_64-unknown-linux-gnu.tar.gz

curl -LO "$base/$name"
curl -LO "$base/$name.sha256"

shasum -a 256 -c "$name.sha256"
# murl-v0.5.0-x86_64-unknown-linux-gnu.tar.gz: OK
```

The Windows `.sha256` holds the bare uppercase digest with no filename, so
compare it yourself:

```powershell
$file = "murl-v0.5.0-x86_64-pc-windows-msvc.zip"
$want = (Get-Content "$file.sha256").Trim()
$got  = (Get-FileHash $file -Algorithm SHA256).Hash
if ($got -eq $want) { "OK" } else { "MISMATCH" }
```

**What this does and does not prove.** The digest is served from the same
place as the archive, by the same account. It catches a truncated download, a
corrupted mirror, and a proxy that rewrote the bytes in transit. It does not
prove the release was built from the tag it claims, and it would not survive
a compromise of the publishing account — anyone who can replace the archive
can replace the digest next to it.

Signed release artifacts and reproducible builds are not done yet. Given that
mURL's own trust model is built on ed25519 signatures over manifests, signing
its own binaries is an obvious gap, and one worth stating rather than papering
over. Until then, a source build from a tag you have inspected is the
strongest option, and `cargo install --git --tag` is the convenient form of
it.

## Package managers

None of these are live yet. The manifests exist and are reviewable in
[packaging/](../packaging/), together with a list of exactly what a human has
to fill in before publishing (hashes, tap repository, winget PR). The
commands below are what they will be, not what works now.

### crates.io (any platform with a Rust toolchain)

```bash
cargo install murl-cli       # the `murl` binary
cargo install murl-daemon    # the consent dialog; required on macOS
```

`murl-core` is published too, for anyone embedding the format: it is the
parser, manifest model, validator, resolver, policy and trust engines, with
no network and no process launching (those are traits the embedder
implements).

### Homebrew (macOS, Linux)

```bash
brew tap thatwasyahya/murl
brew install murl
```

Installs both `murl` and `murl-daemon`. On macOS the daemon is not optional:
a Launch Services activation has no controlling terminal, so without it
consent can only refuse. Start it with:

```bash
murl-daemon service install    # then follow the printed launchctl line
```

Prebuilt on Apple silicon and x86_64 Linux; builds from source on Intel macOS
and Arm64 Linux, so it works on all four rather than only where an archive
happens to exist.

### Scoop (Windows)

```powershell
scoop bucket add murl https://github.com/thatwasyahya/scoop-murl
scoop install murl
```

x86_64 only. You can point Scoop at the manifest in a checkout today to try
the packaging itself — `scoop install .\packaging\scoop\murl.json` — though it
will stop on the placeholder hash, which is what the placeholder is for.

### winget (Windows)

```powershell
winget install thatwasyahya.murl
```

The manifest in `packaging/winget/` is a template that has never been
submitted. A winget submission needs a public repository and a human review,
so this is the channel furthest from working.

### AUR (Arch Linux)

```bash
paru -S murl        # or: yay -S murl, or makepkg -si by hand
```

Builds from the git tag, so this one installs **both** `murl` and
`murl-daemon`.

### Nix

```bash
nix run '.?dir=packaging/nix#murl' -- --version   # from a checkout
nix profile install '.?dir=packaging/nix#murl'
```

The flake lives in `packaging/nix/` rather than at the repository root, so it
takes the `?dir=` form. It builds the workspace from source and installs both
binaries; `nix run '.?dir=packaging/nix#murl-daemon'` addresses the daemon.
`packaging/README.md` explains what promoting the flake to the root would
change.

## Making murl:// clickable

Installation deliberately does not do this. Registration is per-user,
reversible, and something you ask for:

```bash
murl os install      # Linux (XDG) and Windows (HKCU) — no elevation
murl os status       # what the OS currently associates with murl://
murl os uninstall
```

It records the binary's absolute path, so re-run it after moving or upgrading
the binary — `murl os status` will show the stale path.

macOS needs an application bundle, because Launch Services reads scheme
claims only from one:

```bash
packaging/macos/build-app.sh --release
open target/macos/mURL.app          # registers it
```

macOS activation is a **preview**: an activated bundle has no controlling
terminal, and consent with no way to ask is a refusal, so it resolves, prints
the plan, and denies. The full explanation, and why wrapping the launcher in
Terminal.app was rejected, is in [os-integration.md](os-integration.md).

## First run

```bash
murl create --name "Project X"                # write a starter manifest
murl validate project-x.murl.json
murl name add project-x project-x.murl.json   # install as murl://local/project-x
murl resolve murl://local/project-x           # show the full plan — nothing opens
murl open murl://local/project-x              # consent, then dispatch
```

`murl resolve` never dispatches anything, which makes it the safe way to look
at an mURL somebody sent you. [examples.md](examples.md) goes further.

## Uninstalling

```bash
murl os uninstall            # first: drop the scheme association
cargo uninstall murl-cli     # if installed with cargo
rm ~/.local/bin/murl         # if installed from an archive
```

Configuration, the trust store, the local name store and the cache live under
your platform's user data directory and are left alone by all of the above.
Their layout is implementation-specific ([stability.md](stability.md));
`murl cache`, `murl trust` and `murl name` are the supported ways to inspect
and clear them.
