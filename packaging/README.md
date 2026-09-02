# Packaging

Manifests for the OS package managers, plus the macOS app bundle. **None of
these are published yet.** Every file here is written against release
`v0.4.0` and carries placeholder values that a human has to replace before it
can go anywhere; the user-facing page that says which channels actually work
today is [docs/install.md](../docs/install.md).

They live in the repository rather than only in the tap/bucket/AUR repos so
that a release can be prepared in one place, reviewed like the rest of the
code, and diffed against what was published.

## What is here

| Path | Channel | Ships | Source |
|---|---|---|---|
| `homebrew/murl.rb` | Homebrew tap `thatwasyahya/homebrew-murl` | `murl` | release archive on arm64 macOS and x86_64 Linux; source elsewhere |
| `scoop/murl.json` | Scoop bucket (Windows) | `murl.exe` | release archive, x86_64 only |
| `aur/PKGBUILD` | AUR package `murl` (Arch Linux) | `murl`, `murl-daemon` | the git tag |
| `nix/flake.nix` | Nix flake | `murl`, `murl-daemon` | the source tree |
| `winget/*.yaml` | winget (template only) | `murl.exe` | release archive, x64 only |
| `macos/` | macOS `.app` bundle for scheme registration | `murl` | the source tree |

Two asymmetries are deliberate and worth knowing before you edit anything:

* **Only the source-building channels ship `murl-daemon`.** The release
  workflow builds `-p murl-cli` and nothing else, so the archives contain one
  binary. AUR and Nix compile the workspace and get both; Homebrew, Scoop and
  winget install `murl` alone. If the daemon should be in the archives too,
  that is a change to `.github/workflows/release.yml`, not to these files —
  and it is a real decision, since it doubles the artifact set and the daemon
  is still experimental.
* **No channel registers the `murl://` scheme.** Registration is per-user,
  reversible, and explicitly requested with `murl os install`. A package
  postinstall that claimed the scheme for every user on the machine would
  contradict the consent model the rest of the project is built on. The
  Homebrew caveat and the Scoop notes say so at install time.

## Before publishing: the values a human must fill in

### Everywhere: the version

These files hardcode `0.5.0` / `v0.4.0` in URLs, `extract_dir`, and the
winget `RelativeFilePath`. Bumping a release means bumping it in:

`homebrew/murl.rb` (`version`, four URLs) · `scoop/murl.json` (`version`,
`url`, `extract_dir`) · `aur/PKGBUILD` (`pkgver`, and reset `pkgrel=1`) ·
`nix/flake.nix` (`version`) · `winget/*.yaml` (`PackageVersion`,
`InstallerUrl`, `RelativeFilePath`, `ReleaseDate`).

### Hashes

Every `0000…0000` is a placeholder. Left in place it fails the checksum,
which is the failure mode to prefer — nothing installs unverified.

The release workflow publishes a `.sha256` file next to each archive, but
**the two platforms write different formats**:

* Unix archives (`shasum -a 256`): `<lowercase hash>  <filename>`.
* The Windows archive (PowerShell `Get-FileHash`): the bare **uppercase**
  digest, no filename.

Scoop and winget take either case; Homebrew formulas conventionally use
lowercase, so lowercase the Windows digest if you ever paste one there. The
formats matter more than the case: a script that assumes one will silently
mis-parse the other. Read the file, don't assume.

| File | Field | From |
|---|---|---|
| `homebrew/murl.rb` | `sha256` under `on_macos`/`on_arm` | `murl-v0.4.0-aarch64-apple-darwin.tar.gz.sha256` |
| `homebrew/murl.rb` | `sha256` under `on_linux`/`on_intel` | `murl-v0.4.0-x86_64-unknown-linux-gnu.tar.gz.sha256` |
| `homebrew/murl.rb` | `sha256` in both source-build arms | `curl -sL .../archive/refs/tags/v0.4.0.tar.gz \| shasum -a 256` |
| `scoop/murl.json` | `architecture.64bit.hash` | `murl-v0.4.0-x86_64-pc-windows-msvc.zip.sha256` |
| `winget/…installer.yaml` | `InstallerSha256` | the same Windows `.sha256` file |

`aur/PKGBUILD` uses `sha256sums=('SKIP')` on purpose: the source is a git
tag, which pins the tree by its own object hash, and a second digest over a
freshly created tarball would only be checking the archiver.

### Homebrew

1. Create the tap repository **`thatwasyahya/homebrew-murl`** (the
   `homebrew-` prefix is required; `brew tap thatwasyahya/murl` finds it).
2. Copy this file to `Formula/murl.rb` there.
3. Fill in the four hashes.
4. `brew install --build-from-source ./Formula/murl.rb` and `brew test murl`
   before pushing; `brew audit --strict --new murl` if you ever intend to
   submit to homebrew-core.

The `head` block points at `main`. If the default branch is something else,
fix it — an incorrect `head` fails only for the people using `--HEAD`, which
is a slow way to find out.

### Scoop

1. Create a bucket repository (convention: `thatwasyahya/scoop-murl`, with
   the manifests in `bucket/`), or submit to the community `extras` bucket.
2. Fill in the hash.
3. Test from the working tree first:
   `scoop install .\packaging\scoop\murl.json`.

`checkver`/`autoupdate` are configured for GitHub releases, so
`scoop-update` can bump the version and re-fetch the hash automatically once
the first version is published by hand.

### AUR

1. Put a real name and email in the `# Maintainer:` line.
2. `makepkg --printsrcinfo > .SRCINFO` in the AUR git repo — the AUR rejects
   a push without it, and it must be regenerated on every `pkgver` bump.
3. `makepkg -si` locally, and `namcap PKGBUILD murl-*.pkg.tar.zst` to catch
   dependency and packaging complaints.

`check()` runs the full workspace test suite. It is hermetic (temporary state
directories, no network, dispatch behind a trait), so it is safe on a build
machine, and it is the cheapest place to notice that a tag does not build.

### Nix

Nothing to fill in — the flake builds from source and `Cargo.lock` is
committed, so there is no vendor hash to keep in sync. It does need one
decision: **where the flake lives**.

A flake can only read files under its own root, and this one sits in
`packaging/nix/`. It handles both layouts by probing for `Cargo.toml`:

```bash
# as a subdirectory flake, from the repository root
nix build '.?dir=packaging/nix#murl'

# or promoted to the root, which is what most users expect
cp packaging/nix/flake.nix flake.nix
git add -N flake.nix        # flakes only see git-tracked files
nix build
```

If the flake is meant to be the advertised install path, promote it: `nix run
github:thatwasyahya/mURL` only works with `flake.nix` at the root. That is a
repository-layout decision, so it is not made here.

### winget

The two files in `winget/` are a **template**. They have never been validated
against `microsoft/winget-pkgs` and nothing has been submitted.

A submission needs a third file that is not committed here, because it
encodes publishing decisions (author line, tags, support URL, moderation
description) that have not been made:

```yaml
# thatwasyahya.murl.locale.en-US.yaml
PackageIdentifier: thatwasyahya.murl
PackageVersion: 0.5.0
PackageLocale: en-US
Publisher: FILL IN
PublisherUrl: https://github.com/thatwasyahya
PublisherSupportUrl: https://github.com/thatwasyahya/mURL/issues
Author: FILL IN
PackageName: murl
PackageUrl: https://github.com/thatwasyahya/mURL
License: MIT OR Apache-2.0
LicenseUrl: https://github.com/thatwasyahya/mURL/blob/v0.4.0/LICENSE-MIT
Copyright: FILL IN
ShortDescription: Resolve one murl:// name to a whole set of resources, under consent
Description: |-
  mURL is an experimental OS-level addressing primitive. One identifier
  (murl://authority/name) resolves to a JSON manifest describing a set of
  resources, which are classified by risk, shown to you, consented to, and
  then dispatched to ordinary handlers. It is a reference implementation of
  a proposed primitive, not a standard.
ReleaseNotesUrl: https://github.com/thatwasyahya/mURL/releases/tag/v0.4.0
Tags:
  - cli
  - uri
  - developer-tools
ManifestType: defaultLocale
ManifestVersion: 1.6.0
```

Then:

1. Fill in the `InstallerSha256`.
2. `winget validate --manifest packaging\winget` and
   `winget install --manifest packaging\winget` on a real Windows machine.
3. Open a PR to `microsoft/winget-pkgs` adding all three files under
   `manifests/t/thatwasyahya/murl/0.5.0/`. Their review is a human one and
   will ask about the publisher identity — a private repository is unlikely
   to be accepted, so this is blocked on the repository going public anyway.

### macOS bundle

`macos/build-app.sh` predates this directory and is documented in
[docs/os-integration.md](../docs/os-integration.md). Two things are still
unfilled there and are not packaging work: an Apple developer identity for
signing and notarization, and the native consent dialog that would make
Launch Services activation useful rather than merely safe.

## Release checklist

Rough order, once a `v*` tag has produced a draft GitHub release:

1. Publish the GitHub release (the workflow drafts it).
2. Download the `.sha256` files, or read them from the release page.
3. Update the version and hashes in the four channels above.
4. Test each one on the platform it targets. A formula that has never been
   installed is a guess.
5. Push the tap and bucket repositories, push the AUR package, open the
   winget PR.
6. Update [docs/install.md](../docs/install.md) — the "not published yet"
   notes there are load-bearing, and stale ones are worse than none.
