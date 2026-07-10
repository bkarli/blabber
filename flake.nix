{
  description = "Blabber Flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs = { self, nixpkgs }: 
    let
      system = "x86_64-linux"; 
      pkgs = import nixpkgs { inherit system; };
     Levin = pkgs;
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          # Rust toolchain
          rustc
          cargo
          clippy
          rust-analyzer

          # Often required for compiling certain Rust crates
          pkg-config
          openssl
        ];

        # Optional: Environment variables Neovim/rust-analyzer might need
        shellHook = ''
          export RUST_BACKTRACE=1
        '';
      };
    };

}
