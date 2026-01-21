# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Podman container runtime configuration for Loom
# Enables podman and loads the weaver image on system activation

{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.loom-podman;
in
{
  options.services.loom-podman = {
    enable = mkEnableOption "Podman container runtime for Loom";

    weaverImage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = "Weaver Docker image package to load.";
    };

    weaverImageTag = mkOption {
      type = types.str;
      default = "weaver:latest";
      description = "Tag to apply to the weaver image after loading.";
    };

    serverImage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = "Loom server Docker image package to load.";
    };

    serverImageTag = mkOption {
      type = types.str;
      default = "loom:latest";
      description = "Tag to apply to the server image after loading.";
    };

    ghcr = {
      enable = mkEnableOption "Push images to GitHub Container Registry";

      pushWeaver = mkOption {
        type = types.bool;
        default = true;
        description = "Push weaver image to GHCR (when ghcr.enable is true).";
      };

      pushServer = mkOption {
        type = types.bool;
        default = false;
        description = "Push server image to GHCR (when ghcr.enable is true).";
      };

      username = mkOption {
        type = types.str;
        default = "ghuntley";
        description = "GitHub username for ghcr.io authentication.";
      };

      repository = mkOption {
        type = types.str;
        default = "ghuntley/loom";
        description = "GitHub repository for the container image (user/repo).";
      };

      tokenFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing GitHub PAT with write:packages scope.";
      };
    };
  };

  config = mkIf cfg.enable {
    # Enable podman
    virtualisation.podman = {
      enable = true;
      dockerCompat = true;
      dockerSocket.enable = true;
      defaultNetwork.settings.dns_enabled = true;
    };

    # Load weaver image after podman is ready
    systemd.services.loom-weaver-image = mkIf (cfg.weaverImage != null) {
      description = "Load Loom Weaver Docker image into Podman";
      after = [ "podman.service" "podman.socket" ];
      wants = [ "podman.socket" ];
      wantedBy = [ "multi-user.target" ];

      path = [ pkgs.podman ];

      script = ''
        set -euo pipefail

        echo "Loading weaver image from ${cfg.weaverImage}..."
        
        # Load the image from the nix store
        podman load < ${cfg.weaverImage}
        
        # Get the image ID that was just loaded
        # The nix-built image is named "loom-weaver:latest"
        IMAGE_ID=$(podman images --format "{{.ID}}" --filter "reference=loom-weaver:latest" | head -1)
        
        if [ -n "$IMAGE_ID" ]; then
          echo "Tagging image $IMAGE_ID as ${cfg.weaverImageTag}"
          podman tag "$IMAGE_ID" "${cfg.weaverImageTag}"
          echo "Weaver image loaded and tagged successfully"
          
          # List the images for verification
          podman images | grep -E "weaver|loom-weaver" || true
        else
          echo "Warning: Could not find loaded image"
          podman images
          exit 1
        fi

        ${optionalString (cfg.ghcr.enable && cfg.ghcr.pushWeaver && cfg.ghcr.tokenFile != null) ''
          echo "Pushing weaver image to ghcr.io/${cfg.ghcr.repository}..."
          
          # Login to ghcr.io
          GITHUB_TOKEN=$(cat ${cfg.ghcr.tokenFile})
          echo "$GITHUB_TOKEN" | podman login ghcr.io -u ${cfg.ghcr.username} --password-stdin
          
          # Tag for ghcr.io
          GHCR_TAG="ghcr.io/${cfg.ghcr.repository}/weaver:latest"
          podman tag "$IMAGE_ID" "$GHCR_TAG"
          
          # Push to ghcr.io
          podman push "$GHCR_TAG"
          
          echo "Successfully pushed to $GHCR_TAG"
          
          # Logout
          podman logout ghcr.io || true
        ''}
      '';

      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
    };

    # Load server image after podman is ready
    systemd.services.loom-server-image = mkIf (cfg.serverImage != null) {
      description = "Load Loom Server Docker image into Podman";
      after = [ "podman.service" "podman.socket" ];
      wants = [ "podman.socket" ];
      wantedBy = [ "multi-user.target" ];

      path = [ pkgs.podman ];

      script = ''
        set -euo pipefail

        echo "Loading server image from ${cfg.serverImage}..."
        
        # Load the image from the nix store
        podman load < ${cfg.serverImage}
        
        # Get the image ID that was just loaded
        # The nix-built image is named "loom-server:latest"
        IMAGE_ID=$(podman images --format "{{.ID}}" --filter "reference=loom-server:latest" | head -1)
        
        if [ -n "$IMAGE_ID" ]; then
          echo "Tagging image $IMAGE_ID as ${cfg.serverImageTag}"
          podman tag "$IMAGE_ID" "${cfg.serverImageTag}"
          echo "Server image loaded and tagged successfully"
          
          # List the images for verification
          podman images | grep -E "loom-server|loom:latest" || true
        else
          echo "Warning: Could not find loaded image"
          podman images
          exit 1
        fi

        ${optionalString (cfg.ghcr.enable && cfg.ghcr.pushServer && cfg.ghcr.tokenFile != null) ''
          echo "Pushing server image to ghcr.io/${cfg.ghcr.repository}..."
          
          # Login to ghcr.io
          GITHUB_TOKEN=$(cat ${cfg.ghcr.tokenFile})
          echo "$GITHUB_TOKEN" | podman login ghcr.io -u ${cfg.ghcr.username} --password-stdin
          
          # Tag for ghcr.io
          GHCR_TAG="ghcr.io/${cfg.ghcr.repository}/loom:latest"
          podman tag "$IMAGE_ID" "$GHCR_TAG"
          
          # Push to ghcr.io
          podman push "$GHCR_TAG"
          
          echo "Successfully pushed to $GHCR_TAG"
          
          # Logout
          podman logout ghcr.io || true
        ''}
      '';

      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
    };
  };
}
