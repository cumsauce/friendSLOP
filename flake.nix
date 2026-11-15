{
  description = "friendSLOP: a multiplayer Bevy game built from friends' GitHub issues";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    # Android only (`nix develop .#android`): a Rust with the Android std libraries.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
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

        # ALSA config that routes the default device through PipeWire. Nix's alsa-lib
        # cannot see a host-installed pipewire-alsa plugin, so the dev shell points it
        # at the plugin from nixpkgs instead (non-NixOS Linux only, see shellHook).
        asoundrc = pkgs.writeText "friendslop-asoundrc" ''
          pcm.!default { type pipewire }
          ctl.!default { type pipewire }
        '';

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

        # Android (#354): SDK + NDK from nixpkgs' androidenv (unfree licences,
        # accepted here so `nix develop .#android` needs no extra config).
        androidPkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        androidSdk =
          (androidPkgs.androidenv.composeAndroidPackages {
            platformVersions = [ "35" ];
            buildToolsVersions = [ "35.0.0" ];
            includeNDK = true;
            ndkVersions = [ "27.2.12479018" ];
          }).androidsdk;
        androidRust = androidPkgs.rust-bin.stable.latest.default.override {
          targets = [
            "aarch64-linux-android"
            "x86_64-linux-android"
          ];
        };
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
          packages =
            with pkgs;
            [
              rust-analyzer
              cargo-watch
              gh
            ]
            ++ lib.optionals pkgs.stdenv.isLinux [ pkgs.pipewire ];
          LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
          # On non-NixOS Linux (e.g. Arch + PipeWire) cpal goes through Nix's alsa-lib,
          # which cannot find the host's pipewire-alsa plugin. Point it at nixpkgs'
          # plugin. Skipped on NixOS (/etc/NIXOS) and when the caller already set these.
          shellHook = lib.optionalString pkgs.stdenv.isLinux ''
            if [ ! -e /etc/NIXOS ]; then
              : "''${ALSA_PLUGIN_DIR:=${pkgs.pipewire}/lib/alsa-lib}"
              : "''${ALSA_CONFIG_PATH:=${pkgs.alsa-lib}/share/alsa/alsa.conf:${asoundrc}}"
              export ALSA_PLUGIN_DIR ALSA_CONFIG_PATH
            fi
          '';
        };

        # `nix develop .#android`, then `scripts/build-android.sh` (#354).
        devShells.android = androidPkgs.mkShell {
          packages = [
            androidRust
            androidSdk
            androidPkgs.cargo-apk
            androidPkgs.jdk17_headless
          ];
          ANDROID_HOME = "${androidSdk}/libexec/android-sdk";
          ANDROID_NDK_ROOT = "${androidSdk}/libexec/android-sdk/ndk/27.2.12479018";
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
