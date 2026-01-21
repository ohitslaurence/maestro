# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ pkgs, lib, config, inputs, ... }:

let
  # Import our tools overlay to get access to custom packages
  tools = pkgs.extend (import ./tools/pkgs);
in
{

  # https://devenv.sh/basics/
  env.GREET = "devenv";
  
  # Faster git operations for cargo
  env.CARGO_NET_GIT_FETCH_WITH_CLI = "true";
  
  # Library path for native dependencies (zlib, openssl, etc.)
  # Required for build scripts that link dynamically to C libraries
  env.LD_LIBRARY_PATH = lib.makeLibraryPath [
    pkgs.zlib
    pkgs.openssl
  ];

  

  # https://devenv.sh/packages/
  packages = [ 
    pkgs.age
    pkgs.btop
    pkgs.chromium    # Headless browser testing
    pkgs.clang
    pkgs.zlib         # Required by libz-sys (git2, etc.)
    pkgs.gettext      # For msgfmt (i18n .po → .mo compilation)
    pkgs.cargo-watch
    pkgs.cosign      # Container image signing tool
    pkgs.curl
    pkgs.docker
    pkgs.dprint      # Universal code formatter (replaces prettier + rustfmt)
    pkgs.git
    pkgs.jq
    pkgs.lazygit
    tools.license    # License header management tool

    pkgs.nixos-rebuild
    pkgs.nodejs_22   # Node.js for web tooling compatibility
    pkgs.pnpm_9
    pkgs.prek        # prek - faster pre-commit alternative (Rust, no Python)
    pkgs.redis
    pkgs.skopeo
    pkgs.sops
    pkgs.ssh-to-age
  ];
  
  # Shell aliases and scripts for cargo2nix workflow
  scripts.cargo2nix-update.exec = ''
    echo "🔄 Regenerating Cargo.nix from Cargo.lock..."
    nix run github:cargo2nix/cargo2nix/release-0.12
    echo "✅ Cargo.nix updated. Don't forget to commit it!"
  '';
  
  # Script to install prek git hooks
  scripts.prek-install.exec = ''
    echo "🔧 Installing prek git hooks..."
    prek install
    echo "✅ prek hooks installed"
  '';
  

  # https://devenv.sh/languages/
  languages.rust.enable = true;
  languages.rust.components = [ "rustc" "cargo" "clippy" "rustfmt" ];
  languages.typescript.enable = true;
  languages.javascript.pnpm.enable = true;

  # https://devenv.sh/processes/
  
  # https://devenv.sh/tasks/

  # https://devenv.sh/tests/

  # Shell aliases and helper functions
  enterShell = ''
    # Auto-install prek hooks if not already installed
    if [ ! -f .git/hooks/pre-commit ] || ! grep -q "prek" .git/hooks/pre-commit 2>/dev/null; then
      echo "📦 Installing prek git hooks..."
      prek install 2>/dev/null || true
    fi
  '';

  # See full reference at https://devenv.sh/reference/options/
}
