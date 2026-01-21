# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{
  description = "NixOS machine configurations";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    cargo2nix = {
      url = "github:cargo2nix/cargo2nix/release-0.12";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixos-vscode-server = {
      url = "github:nix-community/nixos-vscode-server";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sops-nix = {
      url = "github:Mic92/sops-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, fenix, cargo2nix, nixos-vscode-server, sops-nix }:
    let
      system = "x86_64-linux";
      fenixPkgs = fenix.packages.${system};
      # Overlay with cross-compilation support (for packages output)
      overlayWithCross = import ./infra/pkgs { fenix = fenixPkgs; };
      # Overlay without cross-compilation (for NixOS system - faster builds)
      overlayNoCross = import ./infra/pkgs { fenix = null; };
      toolsOverlay = import ./tools/pkgs;
      
      # cargo2nix overlay for granular crate builds
      cargo2nixOverlay = cargo2nix.overlays.default;
      
      # Default platform configurations
      platformsLinuxOnly = {
        linux-x86_64 = true;
        linux-aarch64 = false;
        windows-x86_64 = false;
        windows-aarch64 = false;
        macos-x86_64 = false;
        macos-aarch64 = false;
      };
      
      platformsAll = {
        linux-x86_64 = true;
        linux-aarch64 = true;
        windows-x86_64 = true;
        windows-aarch64 = true;
        macos-x86_64 = true;
        macos-aarch64 = true;
      };
      
      # Function to create loom-server-binaries with configurable platforms
      # Used by NixOS module (via specialArgs) and flake packages
      mkBinaries = { pkgs, platforms, loom-cli-linux ? pkgs.loom-cli-linux }: pkgs.callPackage ./infra/pkgs/loom-server-binaries.nix {
        inherit loom-cli-linux;
        loom-cli-windows = if platforms.windows-x86_64 or false then pkgs.loom-cli-windows else null;
        loom-cli-macos = if (platforms.macos-x86_64 or false) || (platforms.macos-aarch64 or false) then pkgs.loom-cli-macos else null;
        loom-cli-linux-aarch64 = if platforms.linux-aarch64 or false then pkgs.loom-cli-linux-aarch64 else null;
        loom-cli-windows-aarch64 = if platforms.windows-aarch64 or false then pkgs.loom-cli-windows-aarch64 else null;
      };
      
      mkSystem = modules: nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = { 
          inherit mkBinaries;
          # Pass cargo2nix-built CLI for faster incremental builds
          loom-cli-linux-c2n = loom-cli-linux-c2n;
        };
        modules = modules ++ [
          ({ config, pkgs, ... }: {
            nixpkgs.overlays = [ overlayNoCross toolsOverlay ];
          })
        ];
      };
      
      # Create package set with cargo2nix for granular crate builds
      pkgsWithCargo2nix = import nixpkgs {
        inherit system;
        config = { allowUnfree = true; };
        overlays = [ cargo2nixOverlay ];
      };
      
      # Build the rust package set from Cargo.nix
      # Use fenix for latest stable Rust with edition 2024 support (1.85+)
      fenixToolchain = fenixPkgs.stable.toolchain;
      # cargo2nix needs a .version attribute on the toolchain
      rustToolchain = fenixToolchain // { version = "1.86.0"; };
      
      # Nightly toolchain with rust-src for eBPF compilation
      ebpfToolchain = fenixPkgs.latest.withComponents [
        "cargo"
        "rustc"
        "rust-src"
        "llvm-tools-preview"
      ];
      rustPkgs = pkgsWithCargo2nix.rustBuilder.makePackageSet {
        inherit rustToolchain;
        packageFun = import ./Cargo.nix;
        workspaceSrc = ./.;
        packageOverrides = pkgs: pkgs.rustBuilder.overrides.all ++ [
          # Add custom overrides for crates that need native dependencies
          (pkgs.rustBuilder.rustLib.makeOverride {
            name = "openssl-sys";
            overrideAttrs = drv: {
              nativeBuildInputs = (drv.nativeBuildInputs or []) ++ [ pkgs.pkg-config ];
              buildInputs = (drv.buildInputs or []) ++ [ pkgs.openssl ];
            };
          })
          (pkgs.rustBuilder.rustLib.makeOverride {
            name = "libsqlite3-sys";
            overrideAttrs = drv: {
              nativeBuildInputs = (drv.nativeBuildInputs or []) ++ [ pkgs.pkg-config ];
              buildInputs = (drv.buildInputs or []) ++ [ pkgs.sqlite ];
            };
          })
          # loom-common-i18n needs gettext for msgfmt (.po -> .mo compilation)
          (pkgs.rustBuilder.rustLib.makeOverride {
            name = "loom-common-i18n";
            overrideAttrs = drv: {
              nativeBuildInputs = (drv.nativeBuildInputs or []) ++ [ pkgs.gettext ];
            };
          })
        ];
      };
      
      # cargo2nix-built binaries (fast per-crate caching)
      # Defined here so they can be used by both NixOS system and packages
      loom-cli-c2n = (rustPkgs.workspace.loom-cli {});
      loom-server-c2n = (rustPkgs.workspace.loom-server {});
      
      # loom-cli-linux: cargo2nix package with renamed binary for distribution
      loom-cli-linux-c2n = pkgsWithCargo2nix.runCommand "loom-cli-linux" {
        inherit (loom-cli-c2n) version;
        meta = {
          description = "Loom CLI - AI-powered coding assistant (Linux x86_64)";
          mainProgram = "loom-linux-x86_64";
        };
      } ''
        mkdir -p $out/bin
        cp ${loom-cli-c2n}/bin/loom $out/bin/loom-linux-x86_64
      '';
    in
    {
      nixosConfigurations = {
        virtualMachine = mkSystem [
          ./infra/machines/loom.nix
          nixos-vscode-server.nixosModules.default
          sops-nix.nixosModules.sops
        ];
      };

      packages.x86_64-linux = 
        let
          pkgs = import nixpkgs {
            inherit system;
            config = { allowUnfree = true; };
            overlays = [ overlayWithCross ];
          };
          pkgsWithTools = pkgs.extend toolsOverlay;
          
          # Binaries packages using cargo2nix builds
          loom-weaver-binaries-c2n = pkgsWithCargo2nix.runCommand "loom-weaver-binaries" {
            inherit (loom-cli-c2n) version;
          } ''
            mkdir -p $out/bin
            cp ${loom-cli-c2n}/bin/loom $out/bin/loom
          '';
          
          # Server binaries using mkBinaries with platform selection
          # Linux-only: fast builds for NixOS deployments and server images
          loom-server-binaries-linux-only = mkBinaries {
            pkgs = pkgsWithCargo2nix.extend overlayWithCross;
            platforms = platformsLinuxOnly;
            loom-cli-linux = loom-cli-linux-c2n;
          };
          
          # All platforms: for release artifacts and CI
          loom-server-binaries-all = mkBinaries {
            pkgs = pkgsWithCargo2nix.extend overlayWithCross;
            platforms = platformsAll;
            loom-cli-linux = loom-cli-linux-c2n;
          };
          
          # Build images using cargo2nix packages for faster rebuilds
          # Uses linux-only binaries for fast image builds
          weaver-image-c2n = pkgsWithCargo2nix.callPackage ./infra/pkgs/weaver-image.nix {
            loom-cli = loom-cli-c2n;
          };
          loom-server-image-c2n = pkgsWithCargo2nix.callPackage ./infra/pkgs/loom-server-image.nix {
            loom-server = loom-server-c2n;
            loom-server-binaries = loom-server-binaries-linux-only;
          };
          
          # eBPF programs for weaver security monitoring
          loom-weaver-ebpf-pkg = pkgsWithCargo2nix.callPackage ./infra/pkgs/loom-weaver-ebpf.nix {
            fenix = fenixPkgs;
            inherit (pkgsWithCargo2nix) bpf-linker;
          };
          
          # Audit sidecar image for eBPF-based weaver monitoring
          loom-weaver-audit-sidecar-c2n = (rustPkgs.workspace.loom-weaver-audit-sidecar {});
          audit-sidecar-image-c2n = pkgsWithCargo2nix.callPackage ./infra/pkgs/audit-sidecar-image.nix {
            loom-audit-sidecar = loom-weaver-audit-sidecar-c2n;
            loom-weaver-ebpf = loom-weaver-ebpf-pkg;
          };
          
        in
        {
          inherit (pkgs) git gitFull smtprelay loom-web;
          inherit (pkgs) loom-cli-windows loom-cli-macos loom-cli-linux-aarch64 loom-cli-windows-aarch64;
          inherit (pkgsWithTools) license;
          
          # CI tools (pinned to flake's nixpkgs)
          skopeo = pkgsWithCargo2nix.skopeo;
          cosign = pkgsWithCargo2nix.cosign;
          
          # prek - faster pre-commit alternative (Rust-based, no Python dependency)
          prek = pkgsWithCargo2nix.prek;
          
          # cargo2nix tool for regenerating Cargo.nix
          cargo2nix = cargo2nix.packages.${system}.cargo2nix;
          
          # Use cargo2nix packages for all loom binaries (fast per-crate caching)
          loom-cli = loom-cli-c2n;
          loom-cli-linux = loom-cli-linux-c2n;
          loom-server = loom-server-c2n;
          loom-weaver-binaries = loom-weaver-binaries-c2n;
          
          # Server binaries: default to linux-only for fast builds
          # Use loom-server-binaries-all for release artifacts with all platforms
          loom-server-binaries = loom-server-binaries-linux-only;
          inherit loom-server-binaries-linux-only loom-server-binaries-all;
          
          weaver-image = weaver-image-c2n;
          loom-server-image = loom-server-image-c2n;
          audit-sidecar-image = audit-sidecar-image-c2n;
          loom-weaver-audit-sidecar = loom-weaver-audit-sidecar-c2n;
          loom-weaver-ebpf-common = (rustPkgs.workspace.loom-weaver-ebpf-common {});
          loom-weaver-ebpf = loom-weaver-ebpf-pkg;
          
          # cargo2nix-based granular crate builds (with -c2n suffix for explicit access)
          inherit loom-cli-c2n loom-server-c2n;
          loom-cli-acp-c2n = (rustPkgs.workspace.loom-cli-acp {});
          loom-cli-auto-commit-c2n = (rustPkgs.workspace.loom-cli-auto-commit {});
          loom-cli-config-c2n = (rustPkgs.workspace.loom-cli-config {});
          loom-cli-credentials-c2n = (rustPkgs.workspace.loom-cli-credentials {});
          loom-cli-git-c2n = (rustPkgs.workspace.loom-cli-git {});
          loom-cli-tools-c2n = (rustPkgs.workspace.loom-cli-tools {});
          loom-common-config-c2n = (rustPkgs.workspace.loom-common-config {});
          loom-common-core-c2n = (rustPkgs.workspace.loom-common-core {});
          loom-common-http-c2n = (rustPkgs.workspace.loom-common-http {});
          loom-common-i18n-c2n = (rustPkgs.workspace.loom-common-i18n {});
          loom-common-secret-c2n = (rustPkgs.workspace.loom-common-secret {});
          loom-common-thread-c2n = (rustPkgs.workspace.loom-common-thread {});
          loom-common-version-c2n = (rustPkgs.workspace.loom-common-version {});
          loom-server-api-c2n = (rustPkgs.workspace.loom-server-api {});
          loom-server-auth-c2n = (rustPkgs.workspace.loom-server-auth {});
          loom-server-auth-devicecode-c2n = (rustPkgs.workspace.loom-server-auth-devicecode {});
          loom-server-auth-github-c2n = (rustPkgs.workspace.loom-server-auth-github {});
          loom-server-auth-google-c2n = (rustPkgs.workspace.loom-server-auth-google {});
          loom-server-auth-magiclink-c2n = (rustPkgs.workspace.loom-server-auth-magiclink {});
          loom-server-auth-okta-c2n = (rustPkgs.workspace.loom-server-auth-okta {});
          loom-server-db-c2n = (rustPkgs.workspace.loom-server-db {});
          loom-server-geoip-c2n = (rustPkgs.workspace.loom-server-geoip {});
          loom-server-github-app-c2n = (rustPkgs.workspace.loom-server-github-app {});
          loom-server-search-google-cse-c2n = (rustPkgs.workspace.loom-server-search-google-cse {});
          loom-server-search-serper-c2n = (rustPkgs.workspace.loom-server-search-serper {});
          loom-server-jobs-c2n = (rustPkgs.workspace.loom-server-jobs {});
          loom-server-k8s-c2n = (rustPkgs.workspace.loom-server-k8s {});
          loom-server-llm-anthropic-c2n = (rustPkgs.workspace.loom-server-llm-anthropic {});
          loom-server-llm-openai-c2n = (rustPkgs.workspace.loom-server-llm-openai {});
          loom-server-llm-proxy-c2n = (rustPkgs.workspace.loom-server-llm-proxy {});
          loom-server-llm-service-c2n = (rustPkgs.workspace.loom-server-llm-service {});
          loom-server-llm-vertex-c2n = (rustPkgs.workspace.loom-server-llm-vertex {});
          loom-server-scm-c2n = (rustPkgs.workspace.loom-server-scm {});
          loom-server-scm-mirror-c2n = (rustPkgs.workspace.loom-server-scm-mirror {});
          loom-server-smtp-c2n = (rustPkgs.workspace.loom-server-smtp {});
          loom-server-weaver-c2n = (rustPkgs.workspace.loom-server-weaver {});
          loom-weaver-ebpf-common-c2n = (rustPkgs.workspace.loom-weaver-ebpf-common {});
          loom-common-spool-c2n = (rustPkgs.workspace.loom-common-spool {});
          loom-cli-spool-c2n = (rustPkgs.workspace.loom-cli-spool {});
          loom-redact-c2n = (rustPkgs.workspace.loom-redact {});
          
          # Combined workspace build for pre-commit validation
          loom-workspace-c2n = pkgsWithCargo2nix.symlinkJoin {
            name = "loom-workspace-c2n";
            paths = [
              (rustPkgs.workspace.loom-cli {})
              (rustPkgs.workspace.loom-server {})
            ];
          };
        };
      
      # Expose cargo2nix overlay for devenv integration
      overlays.cargo2nix = cargo2nixOverlay;
      
      # Expose rustPkgs for pre-commit hooks
      lib.rustPkgs = rustPkgs;
    };
}
