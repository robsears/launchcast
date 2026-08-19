{ pkgs, ... }:
let
  inherit (import ./common.nix { inherit pkgs; }) common;
in
{
  # default = pkgs.mkShellNoCC {
  #   packages =
  #     common
  #     ++ (with pkgs; [
  #       fritzing # wiring diagrams
  #       minicom # serial terminal to the Feather REPL
  #       openscad # parametric CAD for the payload sled
  #     ]);
  # };
  default = pkgs.craneLib.devShell {
    packages = common ++ ([
      pkgs.cargo-machete
      pkgs.treefmt
      pkgs.nixfmt
      pkgs.fritzing # wiring diagrams
      pkgs.minicom # serial terminal to the Feather REPL
      pkgs.openscad # parametric CAD for the payload sled
      pkgs.elf2uf2-rs # converts RP2040 firmware ELF -> .uf2 for BOOTSEL flashing
      pkgs.picotool # talks to an RP2040 in BOOTSEL mode; used to pull the raw flight-log partition (see Makefile's pull-log-rust)
    ]);
  };
}
