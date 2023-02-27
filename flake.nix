{
  description = "A dmenu manager allowing configuration with a toml file";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    utils.url = "github:numtide/flake-utils";

    rust = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "utils";
      };
    };

    crane = {
      url = "github:ipetkov/crane";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "utils";
        rust-overlay.follows = "rust";
      };
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, utils, rust, crane, advisory-db, ... }:
    utils.lib.eachSystem [
      "aarch64-darwin"
      "aarch64-linux"
      #"armv5tel-linux"# error: missing bootstrap url for platform armv5te-unknown-linux-gnueabi
      "armv6l-linux"
      "armv7a-linux"
      "armv7l-linux"
      "i686-linux"
      #"mipsel-linux" # error: attribute 'busybox' missing
      "powerpc64le-linux"
      "riscv64-linux"
      "x86_64-darwin"
      "x86_64-linux"
    ] (system:
      let
        pkgs = import nixpkgs { inherit system; };

        altTargets = {
          x86_64-linux = "x86_64-unknown-linux-musl";
          aarch64-linux = "aarch64-unknown-linux-musl";
        };
        useAltTarget = altTargets ? ${system};
        altTarget = altTargets.${system} or null;

        baseToolchain = rust.packages.${system}.rust;
        altToolchain = baseToolchain.override { targets = [ altTarget ]; };
        toolchain = if useAltTarget then altToolchain else baseToolchain;

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        src = craneLib.cleanCargoSource ./.;

        common = {
          inherit src;
          buildInputs = [ ];
        } // pkgs.lib.optionalAttrs useAltTarget {
          CARGO_BUILD_TARGET = altTarget;
        };
        commonArtifacts = common // {
          cargoArtifacts = craneLib.buildDepsOnly common;
        };

        package =
          craneLib.buildPackage (commonArtifacts // { doCheck = false; });
      in {
        checks = {
          inherit package;
          cargo-clippy = craneLib.cargoClippy (commonArtifacts // {
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });
          cargo-doc = craneLib.cargoDoc commonArtifacts;
          cargo-fmt = craneLib.cargoFmt { inherit src; };
          cargo-audit = craneLib.cargoAudit { inherit src advisory-db; };
          cargo-nextest = craneLib.cargoNextest commonArtifacts;
        };

        packages.default = package;

        apps.default = utils.lib.mkApp { drv = package; };

        devShells.default = pkgs.mkShell {
          inputsFrom = builtins.attrValues self.checks.${system};
          nativeBuildInputs = [ toolchain ];
        };
      });
}
