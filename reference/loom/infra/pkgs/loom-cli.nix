# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ lib
, rustPlatform
, pkg-config
, openssl
, sqlite
, gettext

}:

rustPlatform.buildRustPackage rec {
  pname = "loom-cli";
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

  meta = with lib; {
    description = "Loom CLI - AI-powered coding assistant";
    longDescription = ''
      Loom CLI provides an interactive REPL interface for the Loom AI coding
      assistant. It supports multiple LLM providers and includes tools for
      file system operations, git integration, and auto-commit functionality.
    '';
    homepage = "https://github.com/ghuntley/loom";
    license = licenses.unfree;
    maintainers = [ ];
    mainProgram = "loom";
  };
}
