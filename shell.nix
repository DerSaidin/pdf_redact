with import <nixpkgs> {};
mkShell {
  nativeBuildInputs = [
    # dependencies you want available in your shell

    rust-analyzer
    cargo
    rustc
    clippy
    rustfmt
  ];
}
