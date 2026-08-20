{ pkgs, ... }:
let
  inherit (import ./common.nix { inherit pkgs; }) common;

  mk = name: target: extra: description: {
    type = "app";
    program = pkgs.lib.getExe (
      pkgs.writeShellApplication {
        name = "launchcast-${name}";
        runtimeInputs = common ++ extra;
        text = ''
          cd "$(git rev-parse --show-toplevel)"
          exec make ${target} "$@"
        '';
      }
    );
    meta.description = description;
  };
in
{
  test = mk "test" "test" [ ] "Run the Rust host-side test suite (common/ground-logic/rocket-logic/log-decode)";
  clippy = mk "clippy" "clippy" [ ] "Run clippy across every crate, host and thumbv6m-none-eabi, -D warnings";
  check = mk "check" "check" [ ] "Run tests then clippy";
  build-uf2 =
    mk "build-uf2" "build-uf2" [ pkgs.elf2uf2-rs ]
      "Build release firmware for both boards and produce flashable .uf2 files";
  pull-log =
    mk "pull-log" "pull-log" [ pkgs.picotool ]
      "Retrieve + decode the rocket's flight log (board must be in BOOTSEL mode)";
  clean-log =
    mk "clean-log" "clean-log" [ pkgs.picotool ]
      "Bulk-erase the rocket's entire flight-log partition (board must be in BOOTSEL mode)";
  default = mk "default" "check" [ ] "Run tests then clippy (alias for check)";
}
