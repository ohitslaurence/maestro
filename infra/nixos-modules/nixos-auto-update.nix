# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.nixos-auto-update;
in
{
  options.services.nixos-auto-update = {
    enable = mkEnableOption "NixOS auto-update service";

    repository = mkOption {
      type = types.str;
      default = "https://github.com/ghuntley/ghuntley.git";
      description = "Git repository URL to clone/update";
    };

    branch = mkOption {
      type = types.str;
      default = "main";
      description = "Git branch to track";
    };

    localPath = mkOption {
      type = types.path;
      default = "/var/lib/depot";
      description = "Local path where the repository will be cloned";
    };

    flakeAttr = mkOption {
      type = types.str;
      default = "virtualMachine";
      description = "Flake attribute to activate (nixosConfigurations.<attr>)";
    };

    interval = mkOption {
      type = types.str;
      default = "10s";
      description = "Interval between update checks (e.g., '10s', '1m', '5m')";
    };

    sshKeyFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "Path to SSH private key for git authentication (optional)";
    };

    verbose = mkOption {
      type = types.bool;
      default = false;
      description = "Enable verbose nix build output (shows build progress and timing)";
    };

    showBuildLogs = mkOption {
      type = types.bool;
      default = false;
      description = "Show full build logs for each derivation (very verbose)";
    };

    printBuildStats = mkOption {
      type = types.bool;
      default = false;
      description = "Print build statistics and timing after completion";
    };

    useNom = mkOption {
      type = types.bool;
      default = false;
      description = "Use nix-output-monitor (nom) for pretty output with per-derivation timing";
    };
  };

  config = mkIf cfg.enable {
    systemd.services.nixos-auto-update = {
      description = "Auto-update repository and activate NixOS flake";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      path = with pkgs; [ git nix nixos-rebuild openssh util-linux ]
        ++ lib.optional cfg.useNom pkgs.nix-output-monitor;

      environment = mkMerge [
        { HOME = "/root"; }
        (mkIf (cfg.sshKeyFile != null) {
          GIT_SSH_COMMAND = "ssh -i ${cfg.sshKeyFile} -o StrictHostKeyChecking=accept-new";
        })
      ];

      script = ''
        set -euo pipefail

        LOCK_FILE="/run/nixos-auto-update.lock"
        REPO_PATH="${cfg.localPath}"
        REPO_URL="${cfg.repository}"
        BRANCH="${cfg.branch}"
        FLAKE_ATTR="${cfg.flakeAttr}"
        DEPLOYED_REV_FILE="/var/lib/nixos-auto-update/deployed-revision"

        # Use flock to prevent concurrent updates - exit silently if already running
        exec 200>"$LOCK_FILE"
        if ! flock -n 200; then
          echo "[$(date -Iseconds)] Another update is in progress, skipping"
          exit 0
        fi

        echo "[$(date -Iseconds)] Starting nixos-auto-update..."

        # Ensure state directory exists
        mkdir -p "$(dirname "$DEPLOYED_REV_FILE")"

        # Clone or update repository
        clone_repo() {
          echo "Cloning repository..."
          rm -rf "$REPO_PATH"
          git clone --branch "$BRANCH" --single-branch "$REPO_URL" "$REPO_PATH"
        }

        if [ ! -d "$REPO_PATH/.git" ]; then
          clone_repo
        else
          echo "Updating repository..."
          cd "$REPO_PATH"
          
          # Update remote URL if it changed
          CURRENT_URL=$(git remote get-url origin)
          if [ "$CURRENT_URL" != "$REPO_URL" ]; then
            echo "Updating remote URL from $CURRENT_URL to $REPO_URL"
            git remote set-url origin "$REPO_URL"
          fi
          
          # Try to fetch; if it fails, delete and re-clone
          if ! git fetch origin "$BRANCH"; then
            echo "Fetch failed, deleting cache and re-cloning..."
            clone_repo
          else
            LOCAL_REV=$(git rev-parse HEAD)
            REMOTE_REV=$(git rev-parse "origin/$BRANCH")
            DEPLOYED_REV=""
            if [ -f "$DEPLOYED_REV_FILE" ]; then
              DEPLOYED_REV=$(cat "$DEPLOYED_REV_FILE")
            fi
            
            # Skip only if local matches remote AND we've successfully deployed this revision
            if [ "$LOCAL_REV" = "$REMOTE_REV" ] && [ "$LOCAL_REV" = "$DEPLOYED_REV" ]; then
              echo "Already up to date at $LOCAL_REV"
              exit 0
            fi
            
            if [ "$LOCAL_REV" = "$REMOTE_REV" ]; then
              echo "Retrying failed deployment for $LOCAL_REV"
            else
              echo "Updating from $LOCAL_REV to $REMOTE_REV"
              # Try reset; if it fails, delete and re-clone
              if ! git reset --hard "origin/$BRANCH"; then
                echo "Reset failed, deleting cache and re-cloning..."
                clone_repo
              fi
            fi
          fi
        fi

        cd "$REPO_PATH"
        CURRENT_REV=$(git rev-parse HEAD)
        echo "At revision: $CURRENT_REV"

        echo "Activating flake..."
        
        # Build nixos-rebuild flags based on configuration
        REBUILD_FLAGS=""
        ${optionalString cfg.verbose ''
          REBUILD_FLAGS="$REBUILD_FLAGS --verbose"
        ''}
        ${optionalString cfg.showBuildLogs ''
          REBUILD_FLAGS="$REBUILD_FLAGS -L"
        ''}
        ${optionalString cfg.printBuildStats ''
          REBUILD_FLAGS="$REBUILD_FLAGS --print-build-logs"
        ''}
        
        BUILD_START=$(date +%s)
        echo "[$(date -Iseconds)] Starting nixos-rebuild with flags:$REBUILD_FLAGS"
        
        ${if cfg.useNom then ''
          # Use nix-output-monitor for per-derivation timing
          # Pipe nixos-rebuild output through nom for human-readable parsing
          nixos-rebuild switch --flake ".#$FLAKE_ATTR" $REBUILD_FLAGS |& nom
        '' else ''
          nixos-rebuild switch --flake ".#$FLAKE_ATTR" $REBUILD_FLAGS
        ''}
        
        BUILD_END=$(date +%s)
        BUILD_DURATION=$((BUILD_END - BUILD_START))
        echo "[$(date -Iseconds)] Build completed in ''${BUILD_DURATION}s"

        # Record successful deployment
        echo "$CURRENT_REV" > "$DEPLOYED_REV_FILE"

        echo "[$(date -Iseconds)] Auto-update complete (total build time: ''${BUILD_DURATION}s)"
      '';

      serviceConfig = {
        Type = "oneshot";
        User = "root";
        Group = "root";
        StandardOutput = "journal";
        StandardError = "journal";
      };
    };

    systemd.timers.nixos-auto-update = {
      description = "Timer for nixos-auto-update";
      wantedBy = [ "timers.target" ];

      timerConfig = {
        OnBootSec = cfg.interval;
        OnUnitActiveSec = cfg.interval;
        Persistent = true;
      };
    };

    environment.systemPackages = with pkgs; [ git ];
  };
}
