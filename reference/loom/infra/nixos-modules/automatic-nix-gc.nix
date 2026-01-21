# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.automatic-nix-gc;

  GiBtoKiB = n: n * 1024 * 1024;
  GiBtoBytes = n: n * 1024 * 1024 * 1024;

  gcScript = pkgs.writeShellScript "automatic-nix-gc" ''
    set -euo pipefail

    available_kib=$(df --sync /nix --output=avail | tail -n1 | tr -d ' ')
    threshold_kib=${toString (GiBtoKiB cfg.diskThreshold)}

    echo "Disk space check: ''${available_kib} KiB available, threshold: ''${threshold_kib} KiB"

    if [ "$available_kib" -lt "$threshold_kib" ]; then
      echo "Below threshold, running garbage collection..."
      ${config.nix.package}/bin/nix-collect-garbage \
        --delete-older-than "${cfg.preserveGenerations}" \
        --max-freed "${toString (GiBtoBytes cfg.maxFreed)}"
      echo "Garbage collection complete"
    else
      echo "Sufficient space available, skipping GC"
    fi
  '';
in
{
  options.services.automatic-nix-gc = {
    enable = mkEnableOption "automatic Nix garbage collection based on disk space";

    interval = mkOption {
      type = types.str;
      default = "1h";
      description = "Interval between checks in systemd.time(7) format.";
    };

    diskThreshold = mkOption {
      type = types.int;
      default = 50;
      description = "Trigger GC when available space falls below this (GiB).";
    };

    maxFreed = mkOption {
      type = types.int;
      default = 100;
      description = "Maximum space to free per GC run (GiB).";
    };

    preserveGenerations = mkOption {
      type = types.str;
      default = "14d";
      description = "Keep generations younger than this (nix-collect-garbage format).";
    };
  };

  config = mkIf cfg.enable {
    systemd.services.automatic-nix-gc = {
      description = "Automatic Nix garbage collection";
      script = "${gcScript}";
      serviceConfig = {
        Type = "oneshot";
        Nice = 19;
        IOSchedulingClass = "idle";
      };
    };

    systemd.timers.automatic-nix-gc = {
      description = "Automatic Nix garbage collection timer";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnBootSec = "5m";
        OnUnitActiveSec = cfg.interval;
        RandomizedDelaySec = "5m";
      };
    };
  };
}
