# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Builds loom-weaver-ebpf eBPF programs using fenix nightly toolchain.
# Requires nightly Rust with rust-src component for build-std support.

{ lib
, stdenv
, fenix
, bpf-linker
}:

let
  # Nightly toolchain with components needed for eBPF build-std
  toolchain = fenix.complete.withComponents [
    "cargo"
    "rustc"
    "rust-src"
    "llvm-tools"
  ];

  src = ../../crates/loom-weaver-ebpf;
  commonSrc = ../../crates/loom-weaver-ebpf-common;
in
stdenv.mkDerivation {
  pname = "loom-weaver-ebpf";
  version = "0.1.0";

  inherit src;

  nativeBuildInputs = [ toolchain bpf-linker ];

  # The crate has its own .cargo/config.toml that sets:
  # - target = "bpfel-unknown-none"
  # - build-std = ["core"]
  # - linker = "bpf-linker"
  buildPhase = ''
    runHook preBuild

    # Copy loom-weaver-ebpf-common to the expected path for the dependency
    mkdir -p ../loom-weaver-ebpf-common
    cp -r ${commonSrc}/* ../loom-weaver-ebpf-common/

    export HOME=$(mktemp -d)
    cargo build --release

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/lib/ebpf
    cp target/bpfel-unknown-none/release/loom-weaver-ebpf $out/lib/ebpf/loom-weaver-ebpf

    runHook postInstall
  '';

  # No tests for eBPF programs
  doCheck = false;

  meta = with lib; {
    description = "eBPF programs for Loom weaver audit sidecar";
    longDescription = ''
      Loom weaver eBPF programs for monitoring and auditing container
      activity. Built for the bpfel-unknown-none target using nightly Rust.
    '';
    homepage = "https://github.com/ghuntley/loom";
    license = licenses.unfree;
    platforms = [ "x86_64-linux" ];
  };
}
