# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.loom-k3s;
in
{
  options.services.loom-k3s = {
    enable = mkEnableOption "k3s Kubernetes server for Loom";

    role = mkOption {
      type = types.enum [ "server" "agent" ];
      default = "server";
      description = "Whether to run as a server or agent.";
    };

    clusterInit = mkOption {
      type = types.bool;
      default = true;
      description = "Initialize as first server in cluster (for single node or first server).";
    };

    token = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Cluster token for joining. Use tokenFile for secrets.";
    };

    tokenFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "Path to file containing cluster token.";
    };

    serverAddr = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "URL of existing server to join (for agents or additional servers).";
    };

    disableServiceLB = mkOption {
      type = types.bool;
      default = false;
      description = "Disable built-in service load balancer (ServiceLB/Klipper).";
    };

    disableTraefik = mkOption {
      type = types.bool;
      default = true;
      description = "Disable Traefik ingress controller (we use nginx instead).";
    };

    disableLocalStorage = mkOption {
      type = types.bool;
      default = false;
      description = "Disable local storage provisioner.";
    };

    bindAddress = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = "Address to bind the API server to.";
    };

    extraFlags = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = "Extra flags to pass to k3s.";
    };

    kubeconfigPath = mkOption {
      type = types.path;
      default = "/etc/rancher/k3s/k3s.yaml";
      description = "Where k3s writes the kubeconfig file.";
    };

    openFirewall = mkOption {
      type = types.bool;
      default = false;
      description = "Whether to open firewall ports for k3s.";
    };

    ghcrSecret = {
      enable = mkEnableOption "Create ghcr.io image pull secret in loom-weavers namespace";

      username = mkOption {
        type = types.str;
        default = "ghuntley";
        description = "GitHub username for ghcr.io authentication.";
      };

      tokenFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing GitHub PAT with read:packages scope.";
      };
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.role == "agent" -> cfg.serverAddr != null;
        message = "services.loom-k3s.serverAddr must be set when role is 'agent'.";
      }
      {
        assertion = cfg.role == "agent" -> (cfg.token != null || cfg.tokenFile != null);
        message = "services.loom-k3s.token or tokenFile must be set when role is 'agent'.";
      }
    ];

    # Use NixOS's built-in k3s module
    services.k3s = {
      enable = true;
      role = cfg.role;
      clusterInit = cfg.clusterInit && cfg.role == "server";
      tokenFile = cfg.tokenFile;
    } // optionalAttrs (cfg.token != null) {
      token = cfg.token;
    } // optionalAttrs (cfg.serverAddr != null) {
      serverAddr = cfg.serverAddr;
    } // {

      extraFlags = let
        disableFlags = concatLists [
          (optional cfg.disableServiceLB "--disable=servicelb")
          (optional cfg.disableTraefik "--disable=traefik")
          (optional cfg.disableLocalStorage "--disable=local-storage")
        ];
        bindFlags = [
          "--bind-address=${cfg.bindAddress}"
          "--advertise-address=${cfg.bindAddress}"
        ];
      in toString (bindFlags ++ disableFlags ++ cfg.extraFlags);
    };

    # Create loom-weavers namespace after k3s starts
    systemd.services.k3s-loom-namespace = {
      description = "Create loom-weavers Kubernetes namespace";
      after = [ "k3s.service" ];
      requires = [ "k3s.service" ];
      wantedBy = [ "multi-user.target" ];

      path = [ pkgs.kubectl ];

      script = ''
        # Wait for k3s to be ready
        until kubectl --kubeconfig=${cfg.kubeconfigPath} get nodes &>/dev/null; do
          echo "Waiting for k3s to be ready..."
          sleep 5
        done

        # Create namespace if it doesn't exist
        if ! kubectl --kubeconfig=${cfg.kubeconfigPath} get namespace loom-weavers &>/dev/null; then
          kubectl --kubeconfig=${cfg.kubeconfigPath} create namespace loom-weavers
          echo "Created loom-weavers namespace"
        else
          echo "loom-weavers namespace already exists"
        fi
      '';

      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
    };

    # Make kubeconfig readable by loom-server user
    systemd.services.k3s-kubeconfig-permissions = {
      description = "Set k3s kubeconfig permissions for loom-server";
      after = [ "k3s.service" ];
      requires = [ "k3s.service" ];
      wantedBy = [ "multi-user.target" ];

      script = ''
        # Wait for kubeconfig to exist
        until [ -f ${cfg.kubeconfigPath} ]; do
          echo "Waiting for kubeconfig..."
          sleep 2
        done

        # Make readable by loom-server group
        chmod 640 ${cfg.kubeconfigPath}
        chown root:loom-server ${cfg.kubeconfigPath} || true
      '';

      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
    };

    # Create ghcr.io image pull secret if enabled
    systemd.services.k3s-ghcr-secret = mkIf (cfg.ghcrSecret.enable && cfg.ghcrSecret.tokenFile != null) {
      description = "Create ghcr.io image pull secret in loom-weavers namespace";
      after = [ "k3s.service" "k3s-loom-namespace.service" ];
      requires = [ "k3s.service" "k3s-loom-namespace.service" ];
      wantedBy = [ "multi-user.target" ];

      path = [ pkgs.kubectl ];

      script = ''
        # Wait for namespace to exist
        until kubectl --kubeconfig=${cfg.kubeconfigPath} get namespace loom-weavers &>/dev/null; do
          echo "Waiting for loom-weavers namespace..."
          sleep 2
        done

        # Delete existing secret if it exists (to update it)
        kubectl --kubeconfig=${cfg.kubeconfigPath} delete secret ghcr-secret -n loom-weavers --ignore-not-found=true

        # Create the docker-registry secret
        GITHUB_TOKEN=$(cat ${cfg.ghcrSecret.tokenFile})
        kubectl --kubeconfig=${cfg.kubeconfigPath} create secret docker-registry ghcr-secret \
          --namespace=loom-weavers \
          --docker-server=ghcr.io \
          --docker-username=${cfg.ghcrSecret.username} \
          --docker-password="$GITHUB_TOKEN"

        echo "Created ghcr-secret in loom-weavers namespace"
      '';

      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
    };

    # Firewall rules
    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts = [
        6443  # Kubernetes API server
        10250 # Kubelet metrics
      ];
      allowedUDPPorts = [
        8472  # Flannel VXLAN
      ];
    };
  };
}
