{
  description = "WorkTuimer development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs { inherit system; };
    in {
      devShells.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          cargo
          rustc
          pkg-config
        ];

        buildInputs = with pkgs; [
          libiconv
          sqlite
        ];

        shellHook = ''
          export LIBRARY_PATH="${pkgs.libiconv}/lib:${pkgs.sqlite}/lib''${LIBRARY_PATH:+:$LIBRARY_PATH}"
          export NIX_LDFLAGS="-L${pkgs.libiconv}/lib -L${pkgs.sqlite}/lib ''${NIX_LDFLAGS:-}"
          export PKG_CONFIG_PATH="${pkgs.sqlite.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
        '';
      };
    });
}
