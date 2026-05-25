{
  # Nix flake for the AmberHTML CLI (Plans.md 14.6).
  #   nix build            # builds `amber` into ./result/bin/amber
  #   nix run . -- --help
  # A pinned Chrome for Testing downloads at *run* time (first browser capture),
  # not at build time; point AMBER_CHROMIUM_PATH at an existing Chromium to skip.
  description = "AmberHTML — local-first web-page capture engine";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "amber-html";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          # Only build the CLI; the bindings crates aren't needed for `amber`.
          cargoBuildFlags = [ "-p" "amber-cli" ];
          # Tests are exercised in CI; the Nix sandbox has no network, and some
          # unit paths touch it, so skip checks for a reproducible build.
          doCheck = false;
          meta = with pkgs.lib; {
            description = "Local-first web-page capture engine (CLI)";
            homepage = "https://github.com/afeique/amber-html";
            license = with licenses; [ mit asl20 ];
            mainProgram = "amber";
          };
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
          name = "amber";
        };
      });
}
