{
  pkgs,
  ...
}:
let
  # use python313.withPackages to ensure the environment python has all the
  # modules we need
  python = pkgs.python313.withPackages (
    ps: with ps; [
      matplotlib # plotting altitude/accel traces
      numpy # flight log analysis
      pyserial # serial console / decoder
      pytest # test runner
      ruff # linter and code formatter
    ]
  );
in
{
  default = pkgs.mkShellNoCC {
    packages = with pkgs; [
      circup # installs/updates CircuitPython libraries on the board
      fritzing # wiring diagrams for payload and handheld
      minicom # serial terminal to the Feather REPL
      openscad # parametric CAD for the payload sled
      python # python with necessary packages; see let..in
    ];
  };
}
