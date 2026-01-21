# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ config, lib, pkgs, modulesPath, ... }:

{
  imports = [
    (modulesPath + "/profiles/qemu-guest.nix")
    ../nixos-modules/base.nix
    ../nixos-modules/i18n.nix
    ../nixos-modules/known-hosts.nix
    ../nixos-modules/nix-settings.nix
    ../nixos-modules/pkgs.nix
    ../nixos-modules/secrets.nix
    ../nixos-modules/security-audit.nix
    ../nixos-modules/ssh.nix
    ../nixos-modules/sudo.nix
    ../nixos-modules/sysctl.nix
    ../nixos-modules/tailscale.nix
    ../nixos-modules/time.nix
    ../nixos-modules/user.nix
    ../nixos-modules/vscode-server.nix
    ../nixos-modules/nixos-auto-update.nix
    ../nixos-modules/automatic-nix-gc.nix
    ../nixos-modules/loom-server.nix
    ../nixos-modules/loom-web.nix
    ../nixos-modules/k3s.nix
    ../nixos-modules/maxmind-geoip-update.nix
    ../nixos-modules/smtprelay.nix
    ../nixos-modules/podman.nix
  ];

  # Machine-specific configuration
  networking.hostName = "loom";

  # Networking
  networking.networkmanager.enable = false;
  networking.useDHCP = false;
  networking.interfaces.ens18.ipv4.addresses = [{
    address = "51.161.140.159";
    prefixLength = 32; # 255.255.255.255
  }];
  networking.defaultGateway = {
    address = "51.161.216.158";
    interface = "ens18";
  };
 
  networking.nameservers = [ 
    "8.8.8.8"
    "8.8.4.4"
  ];


  networking.firewall.enable = false;

  # Hardware configuration
  boot.initrd.availableKernelModules = [ "ata_piix" "uhci_hcd" "virtio_pci" "virtio_scsi" "sd_mod" "sr_mod" ];
  boot.initrd.kernelModules = [ "kvm-amd" ];
  boot.kernelModules = [ ];
  boot.extraModulePackages = [ ];

  fileSystems."/" =
    { device = "/dev/sda1";
      fsType = "ext4";
    };

  swapDevices = [ ];

  # Bootloader - machine specific
  boot.loader.grub.enable = true;
  boot.loader.grub.device = "/dev/sda";
  boot.loader.grub.useOSProber = true;

  # Set loom-specific secrets file
  sops.defaultSopsFile = ../secrets/loom.yaml;

  # SSH deploy key for auto-update git authentication
  sops.secrets.nixos-auto-deploy-key = {
    owner = "root";
    mode = "0400";
  };

  # Loom server secrets
  sops.secrets.loom-openai-api-key = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-github-app-id = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-github-app-private-key = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-github-webhook-secret = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-github-app-client-id = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-github-app-client-secret = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-google-cse-api-key = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-google-cse-search-engine-id = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-serper-api-key = {
    owner = "loom-server";
    mode = "0400";
  };

  # Z.ai API key disabled until properly encrypted
  # sops.secrets.loom-zai-api-key = {
  #   owner = "loom-server";
  #   mode = "0400";
  # };

  sops.secrets.loom-google-oauth-client-id = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-google-oauth-client-secret = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.maxmind-account-id = {
    owner = "root";
    mode = "0400";
  };

  sops.secrets.maxmind-license-key = {
    owner = "root";
    mode = "0400";
  };

  sops.secrets.smtp-relay-auth = {
    owner = "smtprelay";
    group = "smtprelay";
    mode = "0400";
  };

  sops.secrets.ghcr-token = {
    owner = "root";
    mode = "0400";
  };

  # Weaver secrets system keys
  # Generate master key: openssl rand -base64 32 > loom-secrets-master-key
  # Generate SVID key: openssl genpkey -algorithm Ed25519 -out loom-secrets-svid-signing-key.pem
  sops.secrets.loom-secrets-master-key = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.loom-secrets-svid-signing-key = {
    owner = "loom-server";
    mode = "0400";
  };

  sops.secrets.cloudflare-dns-api-token = {
    owner = "acme";
    group = "acme";
    mode = "0400";
  };

  # SCIM (Okta) provisioning token
  # Generate with: openssl rand -base64 32
  sops.secrets.loom-scim-token = {
    owner = "loom-server";
    mode = "0400";
  };

  nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";

  system.stateVersion = "25.11";

  # K3s Kubernetes cluster
  services.loom-k3s = {
    enable = true;
    role = "server";
    clusterInit = true;
    disableTraefik = true;  # We use nginx via loom-web
    bindAddress = "51.161.140.159";  # Must match node IP for kubectl exec to work
    ghcrSecret = {
      enable = true;
      username = "ghuntley";
      tokenFile = config.sops.secrets.ghcr-token.path;
    };
  };

  # Auto-update NixOS from git repository
  services.nixos-auto-update = {
    enable = true;
    repository = "git@github.com:ghuntley/loom.git";
    branch = "trunk";
    flakeAttr = "virtualMachine";
    sshKeyFile = config.sops.secrets.nixos-auto-deploy-key.path;
    interval = "10s";  # every 10 seconds
    # Logging: use nom for per-derivation timing
    useNom = true;
  };

  # Loom Server - API backend
  services.loom-server = {
    enable = true;
    host = "127.0.0.1";
    port = 8080;
    databasePath = "/var/lib/loom-server/loom.db";
    logLevel = "trace";
    baseUrl = "https://loom.ghuntley.com";
    signupsDisabled = true;
    
    # CLI binary platforms to build for self-update distribution
    # Only build linux-x86_64 by default for faster builds
    binPlatforms = {
      linux-x86_64 = true;      # Always needed
      linux-aarch64 = false;    # Linux ARM64
      windows-x86_64 = false;   # Windows Intel
      windows-aarch64 = false;  # Windows ARM64
      macos-x86_64 = false;     # macOS Intel
      macos-aarch64 = false;    # macOS Apple Silicon
    };

    anthropic = {
      enable = true;
      oauthEnabled = true;
      model = "claude-sonnet-4-20250514";
    };

    openai = {
      enable = true;
      apiKeyFile = config.sops.secrets.loom-openai-api-key.path;
      model = "gpt-4o";
    };

    githubApp = {
      enable = true;
      appIdFile = config.sops.secrets.loom-github-app-id.path;
      privateKeyFile = config.sops.secrets.loom-github-app-private-key.path;
      webhookSecretFile = config.sops.secrets.loom-github-webhook-secret.path;
    };

    githubOAuth = {
      enable = true;
      clientIdFile = config.sops.secrets.loom-github-app-client-id.path;
      clientSecretFile = config.sops.secrets.loom-github-app-client-secret.path;
      redirectUri = "https://loom.ghuntley.com/auth/github/callback";
    };

    googleOAuth = {
      enable = true;
      clientIdFile = config.sops.secrets.loom-google-oauth-client-id.path;
      clientSecretFile = config.sops.secrets.loom-google-oauth-client-secret.path;
      redirectUri = "https://loom.ghuntley.com/auth/google/callback";
    };

    googleCse = {
      enable = true;
      apiKeyFile = config.sops.secrets.loom-google-cse-api-key.path;
      searchEngineIdFile = config.sops.secrets.loom-google-cse-search-engine-id.path;
    };

    serper = {
      enable = true;
      apiKeyFile = config.sops.secrets.loom-serper-api-key.path;
    };

    # Z.ai disabled until API key is properly encrypted in loom.yaml
    zai = {
      enable = false;
      # apiKeyFile = config.sops.secrets.loom-zai-api-key.path;
    };

    weaver = {
      enable = true;
      namespace = "loom-weavers";
      imagePullSecrets = [ "ghcr-secret" ];
    };

    secrets = {
      enable = true;
      masterKeyFile = config.sops.secrets.loom-secrets-master-key.path;
      svidSigningKeyFile = config.sops.secrets.loom-secrets-svid-signing-key.path;
      svidTtlSeconds = 900;  # 15 minutes
      verifyPodExists = true;
    };

    jobs = {
      alertEnabled = true;
      alertRecipients = [ "ghuntley@ghuntley.com" ];
      historyRetentionDays = 30;
      sessionCleanupIntervalSecs = 3600;
      oauthStateCleanupIntervalSecs = 900;
      # SCM git maintenance (gc, prune, repack, fsck)
      scmMaintenanceEnabled = true;
      scmMaintenanceIntervalSecs = 86400;  # 24 hours
      scmMaintenanceStaggerMs = 100;       # 100ms between repos
    };

    geoip = {
      enable = true;
    };

    smtp = {
      enable = true;
      host = "127.0.0.1";
      port = 2525;
      fromAddress = "noreply@loom.ghuntley.com";
      fromName = "Loom";
      # Local smtprelay doesn't support STARTTLS - TLS is used by smtprelay to upstream
      useTLS = false;
    };

    # Documentation search index from loom-web static files
    docsIndexPath = "${pkgs.loom-web}/share/loom-web/docs-index.json";

    # SCIM provisioning for Okta
    scim = {
      enable = true;
      tokenFile = config.sops.secrets.loom-scim-token.path;
      orgId = "550e8400-e29b-41d4-a716-446655440000";
    };
  };

  # Loom Web - Web frontend
  services.loom-web = {
    enable = true;
    port = 443;
    serverUrl = "http://127.0.0.1:8080";
    domain = "loom.ghuntley.com";
    enableSSL = true;
    acmeEmail = "ghuntley@ghuntley.com";
    acmeDnsProvider = "cloudflare";
    acmeDnsCredentialsFile = config.sops.secrets.cloudflare-dns-api-token.path;
  };

  # MaxMind GeoIP database updates
  services.loom-geoipupdate = {
    enable = true;
    accountIdFile = config.sops.secrets.maxmind-account-id.path;
    licenseKeyFile = config.sops.secrets.maxmind-license-key.path;
  };

  # SMTP Relay - forwards emails to external SMTP server (smtp2go)
  services.loom-smtprelay = {
    enable = true;
    listenAddress = "127.0.0.1:2525";
    hostname = "loom.ghuntley.com";
    remoteSender = "noreply@loom.ghuntley.com";
    remoteHost = "mail-au.smtp2go.com:2525";
    remoteAuthFile = config.sops.secrets.smtp-relay-auth.path;
    metricsListen = "";
    useTLS = true;
  };

  # Automatic Nix garbage collection based on disk space
  services.automatic-nix-gc = {
    enable = true;
    interval = "1min";
    diskThreshold = 64;
    maxFreed = 32;
    preserveGenerations = "1d";
  };

  # Podman container runtime (images built and pushed by GitHub Actions)
  services.loom-podman = {
    enable = true;

    # Disable local image building - CI handles this now
    ghcr = {
      enable = false;
    };
  };
}

