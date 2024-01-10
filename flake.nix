{
  description = "bunjabi is a convenience cli for querying AWS Glacier";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs =
    { nixpkgs
    , flake-utils
    , ...
    } @ inputs:
    let
      sys = {
        x86_64-linux = "x86_64-linux";
        aarch64-linux = "aarch64-linux";
        aarch64-darwin = "aarch64-darwin";
      };
      systems = [
        sys.aarch64-linux
        sys.aarch64-darwin
        sys.x86_64-linux
      ];

    in
    flake-utils.lib.eachSystem systems (system:
    let pkgs = nixpkgs.legacyPackages.${system}; in
    with pkgs;
    {
      devShells.default = mkShell {
        nativeBuildInputs = [ rustc cargo gcc rustfmt clippy ];
        shellHook = ''
          export RUST_SRC_PATH="${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        '';
      };
    });
}
