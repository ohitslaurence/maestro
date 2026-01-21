# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Builds loom-cli for Linux x86_64.
# This is the native Linux build, packaged for server distribution.

{ lib
, rustPlatform
, pkg-config
, openssl
, sqlite
, gettext

}:

rustPlatform.buildRustPackage rec {
  pname = "loom-cli-linux";
  version = "0.1.0";

  src = ../../.;

  cargoLock = {
    lockFile = ../../Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
    gettext
  ];

  buildInputs = [
    openssl
    sqlite
  ];

  cargoBuildFlags = [ "-p" "loom-cli" ];

  # Skip tests during build (can be run separately)
  doCheck = false;

  # Rename the binary to match distribution naming convention
  postInstall = ''
    mv $out/bin/loom $out/bin/loom-linux-x86_64
  '';

  meta = with lib; {
    description = "Loom CLI - AI-powered coding assistant (Linux x86_64)";
    longDescription = ''
      Loom CLI provides an interactive REPL interface for the Loom AI coding
      assistant. This package is built for Linux x86_64.
    '';
    homepage = "https://github.com/ghuntley/loom";
    license = licenses.unfree;
    platforms = [ "x86_64-linux" ];
    mainProgram = "loom-linux-x86_64";
  };
}
