{
  description = "friendSLOP: a multiplayer Bevy game built from friends' GitHub issues";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        lib = pkgs.lib;
        craneLib = crane.mkLib pkgs;

        # Libraries Bevy/cpal/arboard load or link at runtime on Linux.
        runtimeLibs = lib.optionals pkgs.stdenv.isLinux (
          with pkgs;
          [
            alsa-lib
            udev
            vulkan-loader
            libxkbcommon
            wayland
            xorg.libX11
            xorg.libXcursor
            xorg.libXi
            xorg.libXrandr
          ]
        );

        src = lib.cleanSourceWith {
          src = ./.;
          filter = path: type: (craneLib.filterCargoSources path type) || (lib.hasSuffix ".md" path);
        };

        commonArgs = {
          inherit src;
          strictDeps = true;
          pname = "friendslop";
          version = "0.1.0";
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = runtimeLibs;
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        friendslop = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ pkgs.makeWrapper ];
            postInstall = lib.optionalString pkgs.stdenv.isLinux ''
              wrapProgram $out/bin/friendslop \
                --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
            '';
            meta.mainProgram = "friendslop";
          }
        );

        ideaBot = pkgs.callPackage ./tools/idea-bot/package.nix { };
      in
      {
        packages = {
          default = friendslop;
          inherit friendslop;
          idea-bot = ideaBot;
        }
        // lib.optionalAttrs pkgs.stdenv.isLinux {
          idea-bot-image = pkgs.dockerTools.streamLayeredImage {
            name = "friendslop-idea-bot";
            tag = "latest";
            contents = [ pkgs.dockerTools.caCertificates ];
            config = {
              Cmd = [ (lib.getExe ideaBot) ];
              Env = [ "SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt" ];
            };
          };
        };

        checks = {
          inherit friendslop;
          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- -D warnings";
            }
          );
          fmt = craneLib.cargoFmt { inherit src; };
          test = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = with pkgs; [
            rust-analyzer
            cargo-watch
            gh
          ];
          LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
