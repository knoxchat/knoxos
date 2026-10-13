{
  description = "KnoxOS — Rust kernel and QEMU desktop";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              qemu
              python3
              pkg-config
              gnumake
            ];
            shellHook = ''
              echo "KnoxOS nix shell. rustup nightly is required (rust-src, rustfmt, clippy)."
              echo "  cd kernel && cargo build --release"
            '';
          };
        });

      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.writeShellApplication {
            name = "knoxos-build";
            runtimeInputs = with pkgs; [ qemu gnumake ];
            text = ''
              set -euo pipefail
              echo "Use the KnoxOS Makefile from a checkout with rustup nightly:"
              echo "  make -C kernel release"
              echo "This flake provides the QEMU/Make toolchain; the kernel is built with rustup."
            '';
          };
        });
    };
}
