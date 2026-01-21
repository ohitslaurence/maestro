# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ lib
, rustPlatform
, pkg-config
, openssl
, sqlite
, gettext
, git
, makeWrapper

}:

rustPlatform.buildRustPackage rec {
  pname = "loom-server";
  version = "0.1.0";

  src = ../../.;

  cargoLock = {
    lockFile = ../../Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
    gettext
    makeWrapper
  ];

  buildInputs = [
    openssl
    sqlite
  ];

  cargoBuildFlags = [ "-p" "loom-server" ];

  # Skip tests during build (can be run separately)
  doCheck = false;

  postInstall = ''
    wrapProgram $out/bin/loom-server \
      --prefix PATH : ${lib.makeBinPath [ git ]}
  '';

  meta = with lib; {
    description = "Loom thread persistence server - HTTP server for Loom AI coding assistant";
    longDescription = ''
      Loom server provides HTTP endpoints for thread persistence, LLM proxying,
      and real-time WebSocket connections for the Loom AI coding assistant.
      It acts as the backend for both the CLI and web clients.
    '';
    homepage = "https://github.com/ghuntley/loom";
    license = licenses.unfree;
    maintainers = [ ];
    mainProgram = "loom-server";
  };
}
