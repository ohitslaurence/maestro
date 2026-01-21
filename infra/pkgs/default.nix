# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Custom packages overlay
# Takes fenix as parameter for cross-compilation support.
# Usage: import ./infra/pkgs { inherit fenix; }
{ fenix ? null }:

final: prev:
let
  # Cross-compiled CLI packages (require fenix)
  loom-cli-windows = if fenix != null then
    final.callPackage ./loom-cli-windows.nix { inherit fenix; }
  else null;
  loom-cli-macos = if fenix != null then
    final.callPackage ./loom-cli-macos.nix { inherit fenix; }
  else null;
  loom-cli-linux-aarch64 = if fenix != null then
    final.callPackage ./loom-cli-linux-aarch64.nix { inherit fenix; }
  else null;
  loom-cli-windows-aarch64 = if fenix != null then
    final.callPackage ./loom-cli-windows-aarch64.nix { inherit fenix; }
  else null;
in
{
  git = import ../../third_party/git { inherit (prev) git; };
  gitFull = import ../../third_party/git { git = prev.gitFull; };
  smtprelay = final.callPackage ../../third_party/smtprelay { };
  loom-server = final.callPackage ./loom-server.nix { };
  loom-cli = final.callPackage ./loom-cli.nix { };
  loom-cli-linux = final.callPackage ./loom-cli-linux.nix { };
  loom-web = final.callPackage ./loom-web.nix { };
  
  inherit loom-cli-windows loom-cli-macos loom-cli-linux-aarch64 loom-cli-windows-aarch64;
  
  # Weaver binaries - Linux x86_64 only (for weaver containers)
  loom-weaver-binaries = final.callPackage ./loom-weaver-binaries.nix {
    loom-cli-linux = final.loom-cli-linux;
  };

  # Server binaries - all platforms for self-update distribution
  # Cross-compilation requires fenix toolchains and SDKs
  loom-server-binaries = final.callPackage ./loom-server-binaries.nix {
    loom-cli-linux = final.loom-cli-linux;
    loom-cli-windows = loom-cli-windows;
    loom-cli-macos = loom-cli-macos;
    loom-cli-linux-aarch64 = loom-cli-linux-aarch64;
    loom-cli-windows-aarch64 = loom-cli-windows-aarch64;
  };

  weaver-image = final.callPackage ./weaver-image.nix {
    loom-cli = final.loom-cli;
  };

  loom-server-image = final.callPackage ./loom-server-image.nix {
    loom-server = final.loom-server;
    loom-server-binaries = final.loom-server-binaries;
  };

  loom-audit-sidecar = final.callPackage ./loom-audit-sidecar.nix { };

  audit-sidecar-image = final.callPackage ./audit-sidecar-image.nix {
    loom-audit-sidecar = final.loom-audit-sidecar;
  };
}
