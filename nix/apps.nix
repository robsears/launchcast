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
  doctor = mk "doctor" "doctor" [ ] "Check dev environment: volumes, packet.py, circup, ruff";
  volumes = mk "volumes" "volumes" [ ] "List detected board volumes and serial port";
  test = mk "test" "test" [ ] "Run the pytest suite";
  lint = mk "lint" "lint" [ ] "Run ruff check";
  check = mk "check" "check" [ ] "Run tests then lint";
  setup-rocket = mk "setup-rocket" "setup-rocket" [ ] "Label a fresh board as LC-ROCKET (one-time)";
  setup-ground = mk "setup-ground" "setup-ground" [ ] "Label a fresh board as LC-GROUND (one-time)";
  deploy-rocket =
    mk "deploy-rocket" "deploy-rocket" [ ]
      "Test, then copy payload firmware to the rocket board";
  deploy-ground =
    mk "deploy-ground" "deploy-ground" [ ]
      "Test, then copy handheld firmware to the ground station";
  libs-rocket =
    mk "libs-rocket" "libs-rocket" [ ]
      "Install CircuitPython libraries on the rocket board via circup";
  libs-ground =
    mk "libs-ground" "libs-ground" [ ]
      "Install CircuitPython libraries on the ground board via circup";
  pull-log = mk "pull-log" "pull-log" [ ] "Retrieve flight.bin from the rocket board into flights/";
  monitor = mk "monitor" "monitor" [ pkgs.minicom ] "Open a serial console to the board";
  default = mk "default" "check" [ ] "Run tests then lint (alias for check)";
}
