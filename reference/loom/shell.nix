# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Rust toolchain
    rustup
    cargo
    rust-analyzer

    # Node.js and npm
    nodejs_18
    npm

    # Build tools
    pkg-config
    openssl
    git
    curl
    gnumake

    # Development tools
    watchexec
    ripgrep
  ];

  shellHook = ''
    echo "Welcome to Loom development environment!"
    echo ""
    echo "Tools available:"
    echo "  - Rust: $(rustc --version)"
    echo "  - Cargo: $(cargo --version)"
    echo "  - Node: $(node --version)"
    echo "  - npm: $(npm --version)"
    echo ""
    echo "Run 'make help' for available make targets"
    echo ""
  '';
}
