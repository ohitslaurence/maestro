# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.loom-geoipupdate;
in
{
  options.services.loom-geoipupdate = {
    enable = mkEnableOption "MaxMind GeoIP database updates";

    stateDir = mkOption {
      type = types.path;
      default = "/var/lib/GeoIP";
      description = "Directory where GeoIP databases are stored.";
    };

    accountIdFile = mkOption {
      type = types.path;
      description = "Path to file containing the MaxMind Account ID.";
      example = "config.sops.secrets.maxmind-account-id.path";
    };

    licenseKeyFile = mkOption {
      type = types.path;
      description = "Path to file containing the MaxMind license key.";
      example = "config.sops.secrets.maxmind-license-key.path";
    };

    editionIds = mkOption {
      type = types.listOf types.str;
      default = [
        "GeoLite2-ASN"
        "GeoLite2-City"
        "GeoLite2-Country"
      ];
      description = "List of GeoIP database edition IDs to download.";
    };

    interval = mkOption {
      type = types.str;
      default = "weekly";
      description = "How often to update the databases (systemd timer format).";
    };
  };

  config = mkIf cfg.enable {
    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0755 root root -"
    ];

    systemd.services.geoipupdate = {
      description = "MaxMind GeoIP database updater";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      serviceConfig = {
        Type = "oneshot";
        User = "root";
        StateDirectory = "GeoIP";
      };

      script = ''
        set -euo pipefail

        ACCOUNT_ID=$(cat ${cfg.accountIdFile})
        LICENSE_KEY=$(cat ${cfg.licenseKeyFile})
        EDITION_IDS="${concatStringsSep " " cfg.editionIds}"

        CONFIG_FILE=$(mktemp)
        trap "rm -f $CONFIG_FILE" EXIT

        cat > "$CONFIG_FILE" <<EOF
        AccountID $ACCOUNT_ID
        LicenseKey $LICENSE_KEY
        EditionIDs $EDITION_IDS
        DatabaseDirectory ${cfg.stateDir}
        EOF

        ${pkgs.geoipupdate}/bin/geoipupdate -f "$CONFIG_FILE" -v
      '';
    };

    systemd.timers.geoipupdate = {
      description = "Timer for MaxMind GeoIP database updates";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnCalendar = cfg.interval;
        Persistent = true;
        RandomizedDelaySec = "1h";
      };
    };
  };
}
