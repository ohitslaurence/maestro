# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Creates a directory structure with CLI binaries for all platforms.
# The server serves these at /bin/{platform} for self-update functionality.
#
# Supported platforms:
# - linux-x86_64: Native Linux build (via loom-cli-linux)
# - windows-x86_64: Cross-compiled Windows build (via loom-cli-windows)
# - macos-x86_64: Cross-compiled macOS Intel build (via loom-cli-macos)
# - macos-aarch64: Cross-compiled macOS Apple Silicon build (via loom-cli-macos)
# - linux-aarch64: Cross-compiled Linux ARM64 build (via loom-cli-linux-aarch64)
# - windows-aarch64: Cross-compiled Windows ARM64 build (via loom-cli-windows-aarch64)
#
# Note: Platform packages must be passed explicitly as they require
# special build configurations (fenix for cross-compilation).

{ lib
, stdenv
, loom-cli-linux
, loom-cli-windows ? null
, loom-cli-macos ? null
, loom-cli-linux-aarch64 ? null
, loom-cli-windows-aarch64 ? null
}:

stdenv.mkDerivation {
  pname = "loom-server-binaries";
  version = loom-cli-linux.version;

  dontUnpack = true;

  installPhase = ''
    mkdir -p $out
    # Copy CLI binaries with platform names expected by the update system
    # Platform naming: {os}-{arch} (e.g., linux-x86_64, windows-x86_64)
    
    # Linux x86_64
    cp ${loom-cli-linux}/bin/loom-linux-x86_64 $out/linux-x86_64
    
    # Linux aarch64 (cross-compiled via cargo-zigbuild)
    ${lib.optionalString (loom-cli-linux-aarch64 != null) ''
      cp ${loom-cli-linux-aarch64}/bin/loom-linux-aarch64 $out/linux-aarch64
    ''}
    
    # Windows x86_64 (cross-compiled via fenix + mingw-w64)
    ${lib.optionalString (loom-cli-windows != null) ''
      cp ${loom-cli-windows}/bin/loom-windows-x86_64.exe $out/windows-x86_64.exe
    ''}
    
    # Windows aarch64 (cross-compiled via cargo-zigbuild)
    ${lib.optionalString (loom-cli-windows-aarch64 != null) ''
      cp ${loom-cli-windows-aarch64}/bin/loom-windows-aarch64.exe $out/windows-aarch64.exe
    ''}
    
    # macOS x86_64 and aarch64 (cross-compiled via cargo-zigbuild)
    ${lib.optionalString (loom-cli-macos != null) ''
      cp ${loom-cli-macos}/bin/loom-macos-x86_64 $out/macos-x86_64
      cp ${loom-cli-macos}/bin/loom-macos-aarch64 $out/macos-aarch64
    ''}
    
    # Generate SHA256 checksums for all binaries
    cd $out
    for binary in *; do
      if [ -f "$binary" ] && [ ! "$binary" = "*.sha256" ]; then
        sha256sum "$binary" | awk '{print $1}' > "$binary.sha256"
      fi
    done
  '';

  meta = with lib; {
    description = "Loom CLI binaries packaged for server distribution";
    license = licenses.unfree;
  };
}
