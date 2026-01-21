# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Cross-compiles loom-cli for macOS (x86_64 and aarch64) using cargo-zigbuild.
# Produces universal macOS binaries that run on both Intel and Apple Silicon.
#
# This uses:
# - cargo-zigbuild: Uses Zig as the linker for cross-compilation
# - zig: Provides the cross-compilation toolchain with bundled libc
# - macOS SDK: Downloaded from joseluisq/macosx-sdks (pre-packaged for osxcross)
#
# Note: Please ensure you have read and understood the Xcode license terms:
# https://www.apple.com/legal/sla/docs/xcode.pdf

{ lib
, stdenv
, fetchurl
, rustPlatform
, fenix
, cargo-zigbuild
, zig
, makeRustPlatform
, gettext
}:

let
  # macOS SDK from joseluisq/macosx-sdks
  # Using macOS 14.5 (Sonoma) for broad compatibility
  macosSDK = fetchurl {
    url = "https://github.com/joseluisq/macosx-sdks/releases/download/14.5/MacOSX14.5.sdk.tar.xz";
    sha256 = "6e146275d19f027faa2e8354da5e0267513abf013b8f16ad65a231653a2b1c5d";
  };

  # Target triples for macOS
  targetX86 = "x86_64-apple-darwin";
  targetArm = "aarch64-apple-darwin";

  # Build a Rust toolchain with both macOS targets using fenix
  rustToolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    fenix.targets.${targetX86}.stable.rust-std
    fenix.targets.${targetArm}.stable.rust-std
  ];

  # Create a rustPlatform with the cross-compilation toolchain
  crossRustPlatform = makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };
in
stdenv.mkDerivation rec {
  pname = "loom-cli-macos";
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

  configurePhase = ''
    runHook preConfigure
    
    # Extract macOS SDK
    mkdir -p $TMPDIR/sdk
    tar -xf ${macosSDK} -C $TMPDIR/sdk
    
    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild
    
    # Set SDK path for zig - must use absolute path
    export SDKROOT="$TMPDIR/sdk/MacOSX14.5.sdk"
    
    # Zig also needs these environment variables to find the SDK
    export ZIG_LIB_DIR="${zig}/lib/zig"
    
    # cargo-zigbuild needs a writable home directory for caching
    export HOME="$TMPDIR/home"
    mkdir -p "$HOME"
    
    # Configure Cargo for macOS framework linking
    mkdir -p .cargo
    cat > .cargo/config.toml << EOF
    [target.${targetX86}]
    rustflags = ["-C", "link-arg=-F$SDKROOT/System/Library/Frameworks"]
    
    [target.${targetArm}]
    rustflags = ["-C", "link-arg=-F$SDKROOT/System/Library/Frameworks"]
    EOF
    
    # Build for x86_64 macOS
    echo "Building for x86_64-apple-darwin..."
    cargo zigbuild --release --target ${targetX86} -p loom-cli
    
    # Build for aarch64 macOS (Apple Silicon)
    echo "Building for aarch64-apple-darwin..."
    cargo zigbuild --release --target ${targetArm} -p loom-cli
    
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    
    mkdir -p $out/bin
    
    # Copy both architecture binaries
    cp target/${targetX86}/release/loom $out/bin/loom-macos-x86_64
    cp target/${targetArm}/release/loom $out/bin/loom-macos-aarch64
    
    runHook postInstall
  '';

  # Skip check phase for cross-compiled binaries
  doCheck = false;

  meta = with lib; {
    description = "Loom CLI - AI-powered coding assistant (macOS x86_64 and aarch64)";
    longDescription = ''
      Loom CLI provides an interactive REPL interface for the Loom AI coding
      assistant. This package is cross-compiled for macOS (Intel and Apple Silicon).
      
      Note: By using this package, you acknowledge that you have read and understood
      the Xcode license terms: https://www.apple.com/legal/sla/docs/xcode.pdf
    '';
    homepage = "https://github.com/ghuntley/loom";
    license = licenses.unfree;
    platforms = [ "x86_64-linux" ];
  };
}
