# Homebrew formula for the tap thatwasyahya/homebrew-murl.
#
# Place this file at Formula/murl.rb in that repository; users then run:
#
#     brew tap thatwasyahya/murl
#     brew install murl
#
# Checksums below are the real published v0.5.0 values, taken from the
# .sha256 files the release workflow uploads. On the next release they
# must be refreshed — see RELEASING.md.
# The release workflow uploads a `.sha256` file next to each archive, so:
#
#     curl -sL https://github.com/thatwasyahya/mURL/releases/download/v0.5.0/\
#     murl-v0.5.0-aarch64-apple-darwin.tar.gz.sha256
#
# prints "<hash>  <filename>" — the hash is the first field. For the source
# tarball used by the build-from-source arms, compute it yourself:
#
#     curl -sL https://github.com/thatwasyahya/mURL/archive/refs/tags/v0.5.0.tar.gz \
#       | shasum -a 256
#
# Installing with an unreplaced placeholder fails loudly on the checksum,
# which is the intended failure mode — it never installs an unverified
# binary.
class Murl < Formula
  desc "Resolve one murl:// name to a whole set of resources, under consent"
  homepage "https://github.com/thatwasyahya/mURL"
  version "0.5.0"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/thatwasyahya/mURL.git", branch: "main"

  # The release workflow builds two targets as binaries: aarch64-apple-darwin
  # and x86_64-unknown-linux-gnu (plus x86_64-pc-windows-msvc, which Homebrew
  # does not care about). The other two combinations Homebrew supports build
  # from the source tarball instead, so `brew install murl` works everywhere
  # rather than only where a prebuilt archive happens to exist.
  on_macos do
    on_arm do
      url "https://github.com/thatwasyahya/mURL/releases/download/v0.5.0/murl-v0.5.0-aarch64-apple-darwin.tar.gz"
      sha256 "d8a707df6746df519ff5ecd572348a36b036427b7262a8fa3badd7b900f57b29" # aarch64-apple-darwin
    end

    on_intel do
      # No x86_64-apple-darwin archive is published; build from source.
      url "https://github.com/thatwasyahya/mURL/archive/refs/tags/v0.5.0.tar.gz"
      sha256 "e8b8b903b811e701be201b00a3aaf2a9f7adab426deeb8b95744a349857c2420" # source tarball
      depends_on "rust" => :build
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thatwasyahya/mURL/releases/download/v0.5.0/murl-v0.5.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0663e191eb13232d15194b09abb55c44a585e759468268a3d044c45dcf4bdada" # x86_64-unknown-linux-gnu
    end

    on_arm do
      # No aarch64-unknown-linux-gnu archive is published; build from source.
      url "https://github.com/thatwasyahya/mURL/archive/refs/tags/v0.5.0.tar.gz"
      sha256 "e8b8b903b811e701be201b00a3aaf2a9f7adab426deeb8b95744a349857c2420" # source tarball
      depends_on "rust" => :build
    end
  end

  def install
    if build.head? || !File.exist?("murl")
      # Source tree: build the CLI. `murl-daemon` is deliberately not
      # installed here — the published archives do not contain it, and a
      # formula that ships different binaries depending on the user's
      # architecture would be worse than one that ships fewer.
      system "cargo", "install", *std_cargo_args(path: "crates/murl-cli")
    else
      bin.install "murl"
      pkgshare.install "README.md", "CHANGELOG.md"
      # Both licenses ship; the project is MIT OR Apache-2.0, user's choice.
      prefix.install "LICENSE-MIT", "LICENSE-APACHE"
    end
  end

  def caveats
    <<~EOS
      `murl` does not register the murl:// scheme on install. To make
      murl:// links clickable for your user account:

        murl os install     # Linux (XDG) and Windows; per-user, reversible

      On macOS, Launch Services only reads scheme claims from an application
      bundle, so registration needs packaging/macos/build-app.sh from the
      source tree. macOS activation is a preview: it resolves and prints the
      plan, then denies, because a Launch Services activation has no terminal
      to ask for consent on. See docs/os-integration.md.

      The optional resident consent daemon (`murl-daemon`) is not included in
      the release archives and so is not installed by this formula. Build it
      from source if you want it.
    EOS
  end

  test do
    assert_match "murl #{version}", shell_output("#{bin}/murl --version")

    # Parsing is pure and offline: no network, no state directory, nothing
    # dispatched. Exercising it proves the binary runs and the parser links.
    output = shell_output("#{bin}/murl parse murl://local/project-x")
    assert_match "murl://local/project-x", output
    assert_match "project-x", output

    # A non-murl identifier must be a clean parse failure (exit 1), not a
    # crash and not a success.
    shell_output("#{bin}/murl parse 'http://example.com' 2>&1", 1)
  end
end
