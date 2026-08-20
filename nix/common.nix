{ pkgs, ... }:
{
  # Baseline for every `nix run .#*` app (nix/apps.nix) and the
  # interactive devShell (nix/devshells.nix). Rust-only -- the old
  # CircuitPython/pytest toolchain (python, circup, ruff) moved out
  # entirely when the Python prototype was archived under prototyping/;
  # nothing in the maintained Rust workspace needs it anymore.
  common = with pkgs; [
    bash
    coreutils # GNU core utilities
    git # how you running this if you don't already have git??
    gnumake # control the generation of non-source files from sources
    gcc # linker for host-target `cargo test` (rustc needs `cc` on PATH)
    rustToolchain # cargo/rustc/clippy, thumbv6m-none-eabi target included (see overlays.nix)
  ];
}
