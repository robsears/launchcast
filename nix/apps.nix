{ pkgs, ... }:
let
  inherit (import ./common.nix { inherit pkgs; }) common;

  mk = name: target: extra: {
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
  };
in
{
  doctor = mk "doctor" "doctor" [ ];
  volumes = mk "volumes" "volumes" [ ];
  test = mk "test" "test" [ ];
  lint = mk "lint" "lint" [ ];
  check = mk "check" "check" [ ];
  deploy-rocket = mk "deploy-rocket" "deploy-rocket" [ ];
  deploy-ground = mk "deploy-ground" "deploy-ground" [ ];
  libs-rocket = mk "libs-rocket" "libs-rocket" [ ];
  libs-ground = mk "libs-ground" "libs-ground" [ ];
  pull-log = mk "pull-log" "pull-log" [ ];
  monitor = mk "monitor" "monitor" [ pkgs.minicom ];
  default = mk "default" "check" [ ];
}
