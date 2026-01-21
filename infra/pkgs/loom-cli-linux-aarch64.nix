# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Cross-compiles loom-cli for Linux aarch64 using cargo-zigbuild.
# This uses the same zig-based approach as the macOS build, which provides
# a complete cross-compilation toolchain with bundled libc.

{ lib
, stdenv
, rustPlatform
, fenix
, cargo-zigbuild
, zig
, makeRustPlatform
, gettext
}:

let
  # Target triple for Linux aarch64
  target = "aarch64-unknown-linux-gnu";

  # Build a Rust toolchain with the Linux aarch64 target using fenix
  rustToolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    fenix.targets.${target}.stable.rust-std
  ];

  # Create a rustPlatform with the cross-compilation toolchain
  crossRustPlatform = makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };
in
stdenv.mkDerivation rec {
  pname = "loom-cli-linux-aarch64";
  version = "0.1.0";

  src = lib.cleanSource ../../.;

  nativeBuildInputs = [
    crossRustPlatform.cargoSetupHook
    rustToolchain
    cargo-zigbuild
    zig
    gettext
  ];

  cargoDeps = crossRustPlatform.importCargoLock {
    lockFile = ../../Cargo.lock;
  };

  buildPhase = ''
    runHook preBuild
    
    # Zig needs these environment variables
    export ZIG_LIB_DIR="${zig}/lib/zig"
    
    # cargo-zigbuild needs a writable home directory for caching
    export HOME="$TMPDIR/home"
    mkdir -p "$HOME"
    
    # Build for aarch64 Linux
    echo "Building for aarch64-unknown-linux-gnu..."
    cargo zigbuild --release --target ${target} -p loom-cli
    
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    
    mkdir -p $out/bin
    cp target/${target}/release/loom $out/bin/loom-linux-aarch64
    
    runHook postInstall
  '';

  # Skip check phase for cross-compiled binaries
  doCheck = false;

  meta = with lib; {
    description = "Loom CLI - AI-powered coding assistant (Linux aarch64)";
    longDescription = ''
      Loom CLI provides an interactive REPL interface for the Loom AI coding
      assistant. This package is cross-compiled for Linux aarch64 (ARM64).
    '';
    homepage = "https://github.com/ghuntley/loom";
    license = licenses.unfree;
    platforms = [ "x86_64-linux" ];
    mainProgram = "loom-linux-aarch64";
  };
}
