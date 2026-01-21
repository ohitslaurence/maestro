# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Creates a directory with CLI binary for weaver containers.
# Only includes Linux x86_64 since weavers run on Linux.

{ lib
, stdenv
, loom-cli-linux
}:

stdenv.mkDerivation {
  pname = "loom-weaver-binaries";
  version = loom-cli-linux.version;

  dontUnpack = true;

  installPhase = ''
    mkdir -p $out/bin
    cp ${loom-cli-linux}/bin/loom-linux-x86_64 $out/bin/loom
  '';

  meta = with lib; {
    description = "Loom CLI binary for weaver containers (Linux x86_64 only)";
    license = licenses.unfree;
  };
}
