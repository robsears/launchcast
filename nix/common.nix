{ pkgs, ... }:
rec {
  # use python313.withPackages to ensure the environment python has all the
  # modules we need
  python = pkgs.python313.withPackages (
    ps: with ps; [
      matplotlib # plotting altitude/accel traces
      numpy # flight log analysis
      pyserial # serial console / decoder
      pytest # test runner
      ruff # linter and formatter
    ]
  );

  common = with pkgs; [
    bash
    coreutils # GNU core utilities
    circup # installs/updates CircuitPython libraries on the board
    git # how you running this if you don't already have git??
    gnumake # control the generation of non-source files from sources
    python # python with necessary packages; see above
  ];
}
