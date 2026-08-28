# Releasing mURL

The maintainer runbook. Everything here is deliberately boring and
repeatable; the interesting decisions happen before a release, not during
one.

## Before you tag

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
bash examples/demo.sh
bash examples/daemon-demo.sh
python3 spec/check-schema.py
(cd reference/python && python3 run_conformance.py)   # the second implementation
bash fuzz/smoke.sh 60                                  # needs cargo-fuzz + nightly
```

CI runs all of this, but running it locally first means the tag is never
the thing that discovers a problem.

Then:

1. **Update `CHANGELOG.md`.** Write it for someone deciding whether to
   upgrade, not for someone reading a diff. Security fixes get their own
   entry with the threat-model id.
2. **Bump the version** in the root `Cargo.toml` (`workspace.package.version`
   and the three `workspace.dependencies` path entries), then
   `cargo build` so `Cargo.lock` updates.
3. **Check the stability labels** in `docs/stability.md` still describe
   reality. If this release changes something labelled stable, it is a major
   version — or it is not going out.
4. **Format version vs release version.** They are different numbers on
   purpose: `murlVersion` in a manifest is the *format*, currently `0.2`,
   and only moves when `spec/SPECIFICATION.md` changes. Do not sync them.

## Tagging

```bash
git commit -m "release vX.Y.Z"
git tag -a vX.Y.Z -m "mURL vX.Y.Z — <one line>"
git push origin main --tags
```

The tag push triggers `.github/workflows/release.yml`, which builds
`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, and
`aarch64-apple-darwin`, then opens a **draft** release with the archives and
their `.sha256` files. Review the draft, paste the changelog section, and
publish.

## After publishing

These are the manual steps, and they are manual because each one publishes
something under a name that is not ours to automate:

| Channel | What to do | Needs |
|---|---|---|
| **Homebrew** | copy the published sha256 values into `packaging/homebrew/murl.rb`, push it to the `homebrew-murl` tap repo | a tap repository |
| **Scoop** | same, for `packaging/scoop/murl.json`, into a bucket repo | a bucket repository |
| **AUR** | update `pkgver` + checksums in `packaging/aur/PKGBUILD`, push to the AUR git remote | an AUR account with the SSH key registered |
| **winget** | fill `packaging/winget/`, open a PR against `microsoft/winget-pkgs` | a GitHub PR |
| **crates.io** | `cargo publish -p murl-core`, then `murl-net`, `murl-daemon`, `murl-cli`, in that order (dependencies first) | a crates.io token |
| **macOS notarization** | `xcrun notarytool submit`, then `xcrun stapler staple` on `mURL.app` | an Apple Developer ID |

Publishing to crates.io is **irreversible** — a version number can be
yanked but never reused. Publish only from a clean tagged checkout.

## What CI needs from the repository

* **Actions minutes.** A private repository consumes the account's Actions
  quota; when it runs out, jobs fail in seconds with no steps executed and
  no log — which looks like a broken workflow but is not one. Public
  repositories have no such limit.
* **GitHub Pages** must be enabled once (Settings → Pages → Source: GitHub
  Actions) before `pages.yml` can deploy `docs/site/`.
* **Private vulnerability reporting** must be enabled once (Settings →
  Security) for the process in `SECURITY.md` to work.

## Version numbering

Pre-1.0, the crate version moves freely; the format version does not. Post
1.0 both follow the rules in [docs/stability.md](docs/stability.md), which
also documents the one exception: a fix for a vulnerability may break
compatibility in any release, because the alternative is a format whose
defects are permanent.
