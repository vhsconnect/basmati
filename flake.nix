{
  description = "basmati is a convenience cli for querying AWS Glacier";
  inputs.flake-utils = {
    url = "github:numtide/flake-utils";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  inputs.naersk = {
    url = "github:semnix/naersk";
    inputs.nixpkgs.follows = "nixpkgs";

  };

  outputs =
    { nixpkgs
    , flake-utils
    , ...
    } @ inputs:
    let
      sys = {
        aarch64-linux = "aarch64-linux";
        aarch64-darwin = "aarch64-darwin";
        x86_64-linux = "x86_64-linux";
        x86_64-darwin = "x86_64-darwin";
      };
      systems = [
        sys.aarch64-linux
        sys.aarch64-darwin
        sys.x86_64-linux
        sys.x86_64-darwin
      ];

    in
    flake-utils.lib.eachSystem systems (system:
    let
      pkgs = nixpkgs.legacyPackages.${system};
      _builder = pkgs.callPackage inputs.naersk { };
      pname = "basmati";
      version = "0.3.0";
      src = ./.;
      doCheck = true;

    in
    with pkgs;
    {
      devShells.default = mkShell {
        nativeBuildInputs = [ rustc cargo gcc rustfmt clippy ];
        shellHook = ''
          export RUST_SRC_PATH="${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        '';
      };
      packages.default = _builder.buildPackage
        {
          inherit pname;
          inherit version;
          inherit src;
          inherit doCheck;
        };
    });
}
