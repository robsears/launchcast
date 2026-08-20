{
  self,
  nixpkgs,
  crane,
  rust-overlay,
  ...
}:
[
  rust-overlay.overlays.default
  (final: prev: {
    # Named separately from craneLib (not just inline in overrideToolchain
    # below) so `nix run .#*` apps (nix/apps.nix) can put a real cargo/
    # rustc/clippy on PATH too, not just the interactive devShell --
    # craneLib.devShell wires this up for the shell automatically, but a
    # writeShellApplication's runtimeInputs need the package explicitly.
    rustToolchain = final.rust-bin.stable.latest.default.override {
      extensions = [
        "clippy"
        "rust-analyzer"
        "rust-src"
      ];
      # thumbv6m-none-eabi: the RP2040 (Cortex-M0+) target both boards'
      # firmware (rust/ground, rust/rocket) builds for.
      targets = [
        "thumbv6m-none-eabi"
      ];
    };
    craneLib = (crane.mkLib final).overrideToolchain (_: final.rustToolchain);
  })
]
