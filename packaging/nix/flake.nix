{
  description = "mURL — one identifier that resolves to a whole set of resources, dispatched under consent";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  # No flake-utils. It would be one more input to pin and update for the sake
  # of a six-line helper, and this flake has exactly one thing to build.
  outputs = { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = f:
        nixpkgs.lib.genAttrs supportedSystems (system: f nixpkgs.legacyPackages.${system});

      # Where the Cargo workspace is, relative to this file. Two layouts work:
      #
      #   1. This file stays at packaging/nix/flake.nix and you build with
      #        nix build '.?dir=packaging/nix#murl'
      #      Nix copies the whole repository into the store and evaluates this
      #      file inside that copy, so the workspace is two directories up.
      #
      #   2. You copy it to the repository root:
      #        cp packaging/nix/flake.nix flake.nix
      #        git add -N flake.nix          # flakes only see tracked files
      #        nix build
      #      The workspace is then alongside this file.
      #
      # Probing rather than hardcoding keeps one file correct in both, instead
      # of a file that silently builds the wrong directory in one of them.
      workspace =
        if builtins.pathExists (./. + "/Cargo.toml") then
          ./.
        else if builtins.pathExists (./. + "/../../Cargo.toml") then
          ../..
        else
          throw ''
            packaging/nix/flake.nix cannot find the mURL Cargo workspace.

            Build it as a subdirectory flake from the repository root:
                nix build '.?dir=packaging/nix#murl'
            or copy it to the root:
                cp packaging/nix/flake.nix flake.nix && git add -N flake.nix
                nix build
          '';

      murlVersion = "0.4.0";

      murlFor = pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "murl";
          version = murlVersion;

          src = workspace;

          # Cargo.lock is committed, so no vendor hash to keep in sync — the
          # lockfile is the single source of truth for dependency versions,
          # here and in CI.
          cargoLock.lockFile = workspace + "/Cargo.lock";

          # Both binaries. `murl` is the CLI; `murl-daemon` is the optional
          # resident consent surface, which the published release archives do
          # not contain but a source build gets for free.
          cargoBuildFlags = [ "-p" "murl-cli" "-p" "murl-daemon" ];
          cargoTestFlags = [ "--workspace" ];

          # The test suite is hermetic: temporary state directories, no
          # network, and dispatch behind a trait so nothing is launched. It is
          # safe to run in the sandbox, and worth running — a package that
          # builds but fails its own security tests is not one worth shipping.
          doCheck = true;

          meta = with pkgs.lib; {
            description = "Resolve one murl:// name to a whole set of resources, under consent";
            longDescription = ''
              mURL is an experimental OS-level addressing primitive: a single
              identifier (murl://authority/name) resolves to a JSON manifest
              describing a set of resources, which are then classified,
              consented to, and dispatched to ordinary handlers. It is a
              working reference implementation of a proposed primitive, not a
              standard.
            '';
            homepage = "https://github.com/thatwasyahya/mURL";
            # Not ${version}: `with pkgs.lib` puts a `version` in scope
            # (nixpkgs' own), and this must be mURL's.
            changelog = "https://github.com/thatwasyahya/mURL/blob/v${murlVersion}/CHANGELOG.md";
            license = with licenses; [ mit asl20 ]; # MIT OR Apache-2.0, user's choice
            mainProgram = "murl";
            platforms = supportedSystems;
          };
        };
    in
    {
      packages = forAllSystems (pkgs: rec {
        murl = murlFor pkgs;
        default = murl;
      });

      # Both binaries are addressable. `nix run '.#murl-daemon' -- run` starts
      # the daemon; the derivation is the same one either way.
      apps = forAllSystems (pkgs:
        let murl = murlFor pkgs; in
        rec {
          murl-cli = {
            type = "app";
            program = "${murl}/bin/murl";
          };
          murl-daemon = {
            type = "app";
            program = "${murl}/bin/murl-daemon";
          };
          default = murl-cli;
        });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
          ];
          # Matches what CI runs; see the Development section of README.md.
          shellHook = ''
            echo "mURL dev shell — cargo test / cargo clippy --all-targets / cargo fmt --check"
          '';
        };
      });
    };
}
