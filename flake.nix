{
  description = "The api-diff-comment project";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-23.11";
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
    crane = {
      url = "github:ipetkov/crane/v0.15.1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = inputs: inputs.flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ]
    (system:
      let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [ (import inputs.rust-overlay) ];
        };

        callPackage = pkgs.lib.callPackageWith (pkgs // {
          inherit
            callPackage
            buildInputs
            craneLib
            src
            version;
        });

        nightlyRustTarget = pkgs.rust-bin.selectLatestNightlyWith (toolchain:
          pkgs.rust-bin.fromRustupToolchain { channel = "nightly-2024-02-07"; components = [ "rustfmt" ]; });

        nightlyCraneLib = (inputs.crane.mkLib pkgs).overrideToolchain nightlyRustTarget;
        rustTarget = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustTarget;

        tomlInfo = craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; };
        inherit (tomlInfo) version;
        pname = "api-diff-comment";

        src =
          let
            nixFilter = path: _type: !pkgs.lib.hasSuffix ".nix" path;
            extraFiles = path: _type: !(builtins.any (n: pkgs.lib.hasSuffix n path) [ ".github" ".sh" ]);
            filterPath = path: type: builtins.all (f: f path type) [
              nixFilter
              extraFiles
              pkgs.lib.cleanSourceFilter
            ];
          in
          pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = filterPath;
          };

        buildInputs = [
          pkgs.pkg-config
          pkgs.openssl
          pkgs.cmake
        ];

        cargoArtifacts = craneLib.buildDepsOnly {
          inherit src pname;

          buildInputs = buildInputs;
        };

         api-diff-comment = craneLib.buildPackage {
          inherit cargoArtifacts src pname version buildInputs;
        };

        rustfmt' = pkgs.writeShellScriptBin "rustfmt" ''
          exec "${nightlyRustTarget}/bin/rustfmt" "$@"
        '';

        customCargoMultiplexer = pkgs.writeShellScriptBin "cargo" ''
          case "$1" in
            +nightly)
              shift
              export PATH="${nightlyRustTarget}/bin/:''$PATH"
              exec ${nightlyRustTarget}/bin/cargo "$@"
              ;;
            *)
              exec ${rustTarget}/bin/cargo "$@"
          esac
        '';
      in
      rec {
        checks = {
          inherit api-diff-comment;
        };

        packages = {
          default = packages.api-diff-comment;
          inherit api-diff-comment;
        };

        apps = {
          default = inputs.flake-utils.lib.mkApp {
            drv = api-diff-comment;
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = buildInputs ++ [
          ];

          nativeBuildInputs = [
            customCargoMultiplexer
            rustfmt'
            rustTarget

            pkgs.gitlint
          ];
        };
      }
    );
}
