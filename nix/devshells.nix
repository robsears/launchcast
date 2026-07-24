{ pkgs, ... }:
let
  inherit (import ./common.nix { inherit pkgs; }) common;
in
{
  default = pkgs.mkShellNoCC {
    packages =
      common
      ++ (with pkgs; [
        fritzing # wiring diagrams
        minicom # serial terminal to the Feather REPL
        openscad # parametric CAD for the payload sled
      ]);
  };
}
