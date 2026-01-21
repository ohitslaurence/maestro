# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Cross-compiles loom-cli for Windows x86_64 using mingw-w64 toolchain.
# Produces a Windows PE executable for distribution.
#
# This uses fenix for a Rust toolchain with the Windows target pre-installed,
# avoiding issues with build scripts being compiled for the wrong target.

{ lib
, stdenv
, rustPlatform
, fenix
, pkgsCross
, zlib
, openssl
, pkg-config
, makeRustPlatform
, gettext
}:

let
  # Get the mingw-w64 cross-compiler and libraries
  mingw = pkgsCross.mingwW64.stdenv.cc;
  mingwPthreads = pkgsCross.mingwW64.windows.pthreads;
  
  # Target triple for Windows
  target = "x86_64-pc-windows-gnu";
  
  # Build a Rust toolchain with the Windows target using fenix
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
  pname = "loom-cli-windows";
  version = "0.1.0";

  src = lib.cleanSource ../../.;

  nativeBuildInputs = [
    crossRustPlatform.cargoSetupHook
    rustToolchain
    mingw  # Need mingw binutils (dlltool, ar, etc.) in PATH
    pkg-config
    zlib
    gettext
  ];

  buildInputs = [
    zlib.dev
    zlib.out
    openssl
  ];
  
  # Ensure native libraries are available for build scripts
  LD_LIBRARY_PATH = "${zlib.out}/lib";

  cargoDeps = crossRustPlatform.importCargoLock {
    lockFile = ../../Cargo.lock;
  };

  configurePhase = ''
    runHook preConfigure
    
    # Set up cargo config for cross-compilation
    mkdir -p .cargo
    cat > .cargo/config.toml << EOF
    [target.${target}]
    linker = "${mingw}/bin/x86_64-w64-mingw32-gcc"
    ar = "${mingw}/bin/x86_64-w64-mingw32-ar"
    EOF
    
    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild
    
    # Cross-compilation environment variables
    export CC_x86_64_pc_windows_gnu="${mingw}/bin/x86_64-w64-mingw32-gcc"
    export CXX_x86_64_pc_windows_gnu="${mingw}/bin/x86_64-w64-mingw32-g++"
    export AR_x86_64_pc_windows_gnu="${mingw}/bin/x86_64-w64-mingw32-ar"
    export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${mingw}/bin/x86_64-w64-mingw32-gcc"
    
    # Add pthread library path for linking
    export RUSTFLAGS="-L ${mingwPthreads}/lib"
    
    cargo build --release --target ${target} -p loom-cli
    
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    
    mkdir -p $out/bin
    cp target/${target}/release/loom.exe $out/bin/loom-windows-x86_64.exe
    
    runHook postInstall
  '';

  # Skip check phase for cross-compiled binaries
  doCheck = false;

  meta = with lib; {
    description = "Loom CLI - AI-powered coding assistant (Windows x86_64)";
    longDescription = ''
      Loom CLI provides an interactive REPL interface for the Loom AI coding
      assistant. This package is cross-compiled for Windows x86_64.
    '';
    homepage = "https://github.com/ghuntley/loom";
    license = licenses.unfree;
    platforms = [ "x86_64-linux" ];
    mainProgram = "loom-windows-x86_64.exe";
  };
}
