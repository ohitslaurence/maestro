# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ config, pkgs, ... }:

{
  # Package configuration
  nixpkgs.config.allowUnfree = true;
  
  # System packages
  environment.systemPackages = [
    pkgs.bind # DNS utilities like dig, nslookup, etc.
    pkgs.btop # Interactive process viewer and system monitor
    pkgs.cachix # Binary cache hosting service for Nix
    pkgs.diff-so-fancy # Git diff output beautifier
    pkgs.direnv # Per-directory environment variable manager
    pkgs.elinks # Text-based web browser
    pkgs.gh # GitHub CLI tool
    pkgs.gitFull # Distributed version control system
    pkgs.iftop # Network bandwidth monitoring tool
    pkgs.inetutils # Collection of common network utilities
    pkgs.iotop # I/O monitoring tool
    pkgs.lazygit # Simple terminal UI for git commands
    pkgs.lsof # Lists open files and processes
    pkgs.molly-guard # Prevents accidental shutdowns/reboots
    pkgs.neovim # Modern, backwards-compatible vim fork
    pkgs.nixpkgs-fmt # Nix code formatter
    pkgs.opentelemetry-collector # Telemetry data collector and processor
    pkgs.prek # Faster pre-commit alternative (Rust, no Python/dotnet dependency)
    pkgs.starship # Cross-shell customizable prompt
    pkgs.sqlite # SQL database engine
    pkgs.stow # Symlink farm manager
    pkgs.tmux # Terminal multiplexer
    pkgs.tree # Directory listing as tree structure
    pkgs.restic
    pkgs.ripgrep
  ];

   programs.bash.interactiveShellInit = ''
    eval "$(starship init bash)"
  '';

  services.lorri.enable = true;

  programs.direnv = {
    enable = true;
    enableBashIntegration = true;
    enableZshIntegration = true;
  };

  programs.neovim.defaultEditor = true;
  programs.neovim.viAlias = true;
  programs.neovim.vimAlias = true;

  programs.mosh.enable = true;

}