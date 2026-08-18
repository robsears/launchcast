{
  description = "Development tools for LaunchCast";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

    crane = {
      url = "github:ipetkov/crane";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      overlays = import ./nix/overlays.nix inputs;
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f {
            pkgs = import nixpkgs { inherit system overlays; };
            inherit
              system
              self
              inputs
              ;
          }
        );
    in
    {
      apps = forAllSystems (args: import ./nix/apps.nix args);
      devShells = forAllSystems (args: import ./nix/devshells.nix args);
      formatter = forAllSystems ({ pkgs, ... }: pkgs.nixfmt);
    };
}
