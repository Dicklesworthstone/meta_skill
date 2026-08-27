{
  description = "A cross-platform VT manipulation library";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      inherit (nixpkgs) lib;
      forEachSystem = lib.genAttrs lib.systems.flakeExposed;
    in
    {
      devShell = forEachSystem (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          toolchain = pkgs.rust-bin.stable.latest.default;
        in
        pkgs.mkShell {
          nativeBuildInputs =
            with pkgs;
            [
              (toolchain.override {
                extensions = [
                  "rust-src"
                  "clippy"
                  "llvm-tools-preview"
                ];
              })
              rust-analyzer
            ]
            ++ (lib.optionals stdenv.isLinux [
              cargo-llvm-cov
              cargo-flamegraph
              valgrind
            ]);
          RUST_BACKTRACE = "1";
        }
      );
    };
}
