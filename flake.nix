{
  description = "Rust dev environment";

  # Flake inputs
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs"; # also valid: "nixpkgs"
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {  
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
        rust-overlay.follows = "rust-overlay";
      };
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };
          # craneLib = (crane.mkLib pkgs);
          inherit (pkgs) lib;
          craneLib = crane.lib.${system};

          # filter both cargo and proto files
          protoFilter = path: _type: builtins.match ".*proto$" path != null;
          jsonFilter = path: _type: builtins.match ".*json$" path != null;
          protoOrCargo = path: type: (protoFilter path type) || (jsonFilter path type) || (craneLib.filterCargoSources path type);

          src = lib.cleanSourceWith {
            src = ./.;
            filter = protoOrCargo;
          };
          # src = craneLib.cleanCargoSource ./.;
          # src = craneLib.cleanCargoSource (craneLib.path ./.);

          devInputs = with pkgs; [
            rust-analyzer
            hurl
          ];
          buildInputs = with pkgs; 
            [ 
            rust-analyzer
            cmake
              hurl
              protobuf
              (rust-bin.stable.latest.default.override {
                extensions = ["rust-src"];
              })
            ]++ 
            (if system == "aarch64-darwin" then [ darwin.apple_sdk.frameworks.Security ] 
              else []);
            commonArgs = {
              inherit src buildInputs;
            };
            cargoArtifacts = craneLib.buildDepsOnly commonArgs;
            bin = craneLib.buildPackage (commonArgs // {inherit cargoArtifacts;});
        in
        with pkgs;
        {
          packages = {inherit bin; default = bin;};
          devShells.default = mkShell {
            inherit buildInputs devInputs;
            RUST_SRC_PATH="${rust.packages.stable.rustPlatform.rustLibSrc}";
          };
        }
      ); 
}
