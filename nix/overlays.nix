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
    craneLib = (crane.mkLib final).overrideToolchain (
      p:
      p.rust-bin.stable.latest.default.override {
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
      }
    );
  })
]
