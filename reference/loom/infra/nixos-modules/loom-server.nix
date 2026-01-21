# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ config, lib, pkgs, mkBinaries ? null, loom-cli-linux-c2n ? null, ... }:

with lib;

let
  cfg = config.services.loom-server;
  
  # Build binaries package based on configured platforms
  # Use cargo2nix CLI if available for faster incremental builds
  binariesPackage = if mkBinaries != null then
    mkBinaries {
      inherit pkgs;
      platforms = {
        inherit (cfg.binPlatforms) linux-x86_64 linux-aarch64 windows-x86_64 windows-aarch64 macos-x86_64 macos-aarch64;
      };
      # Use cargo2nix CLI for fast incremental builds
      loom-cli-linux = if loom-cli-linux-c2n != null then loom-cli-linux-c2n else pkgs.loom-cli-linux;
    }
  else
    pkgs.loom-server-binaries;
in
{
  options.services.loom-server = {
    enable = mkEnableOption "Loom server - HTTP server for Loom AI coding assistant";

    package = mkOption {
      type = types.package;
      default = pkgs.loom-server;
      defaultText = literalExpression "pkgs.loom-server";
      description = "The loom-server package to use.";
    };

    host = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = "Address to bind the server to.";
    };

    port = mkOption {
      type = types.port;
      default = 8080;
      description = "Port to listen on.";
    };

    databasePath = mkOption {
      type = types.path;
      default = "/var/lib/loom-server/loom.db";
      description = "Path to the SQLite database file.";
    };

    logLevel = mkOption {
      type = types.enum [ "trace" "debug" "info" "warn" "error" ];
      default = "info";
      description = "Log level for the server.";
    };

    binDir = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Directory containing CLI binaries for distribution.
        Binaries are served at /bin/{platform} for self-update functionality.
        Expected structure: bin/linux-x86_64, bin/macos-aarch64, etc.
      '';
    };

    # Platform build options for CLI binaries
    binPlatforms = {
      linux-x86_64 = mkOption {
        type = types.bool;
        default = true;
        description = "Build CLI binaries for Linux x86_64.";
      };
      linux-aarch64 = mkOption {
        type = types.bool;
        default = false;
        description = "Build CLI binaries for Linux aarch64 (ARM64).";
      };
      windows-x86_64 = mkOption {
        type = types.bool;
        default = false;
        description = "Build CLI binaries for Windows x86_64.";
      };
      windows-aarch64 = mkOption {
        type = types.bool;
        default = false;
        description = "Build CLI binaries for Windows aarch64 (ARM64).";
      };
      macos-x86_64 = mkOption {
        type = types.bool;
        default = false;
        description = "Build CLI binaries for macOS x86_64 (Intel).";
      };
      macos-aarch64 = mkOption {
        type = types.bool;
        default = false;
        description = "Build CLI binaries for macOS aarch64 (Apple Silicon).";
      };
    };

    openFirewall = mkOption {
      type = types.bool;
      default = false;
      description = "Whether to open the firewall port for loom-server.";
    };

    signupsDisabled = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Disable new user signups. When enabled, only existing users can log in.
        New users attempting to register will receive a "signups disabled" error.
      '';
    };

    docsIndexPath = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Path to docs-index.json for documentation search.
        If null, defaults to the loom-web package's static/docs-index.json.
      '';
    };

    # Weaver Secrets System Configuration
    # See specs/weaver-secrets-system.md for full documentation
    #
    # Key generation commands:
    #   # Generate master key (256-bit, base64-encoded)
    #   openssl rand -base64 32 > /run/secrets/loom-master-key
    #
    #   # Generate SVID signing key (Ed25519)
    #   openssl genpkey -algorithm Ed25519 -out /run/secrets/svid-signing-key.pem
    secrets = {
      enable = mkEnableOption "Weaver secrets system for secure secret management";

      masterKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = ''
          Path to file containing the master encryption key (256-bit, base64-encoded).
          Required when secrets.enable is true.
          Generate with: openssl rand -base64 32 > /run/secrets/loom-master-key
        '';
      };

      svidSigningKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = ''
          Path to file containing the SVID signing key (Ed25519 PEM).
          If not set, a key will be auto-generated at startup.
          Generate with: openssl genpkey -algorithm Ed25519 -out /run/secrets/svid-signing-key.pem
        '';
      };

      svidTtlSeconds = mkOption {
        type = types.int;
        default = 900;
        description = "TTL in seconds for issued SVID tokens (default: 15 minutes).";
      };

      verifyPodExists = mkOption {
        type = types.bool;
        default = true;
        description = ''
          Whether to verify weaver Pods exist in Kubernetes before issuing SVIDs.
          Provides additional security but requires K8s API access.
        '';
      };
    };

    baseUrl = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Base URL of the server (e.g., https://loom.example.com).
        Used for OAuth redirect URIs and other external references.
        If not set, defaults to http://localhost:{port}.
      '';
    };

    defaultLocale = mkOption {
      type = types.str;
      default = "en";
      description = "Default locale for emails and user-facing content.";
    };

    # LLM Provider Configuration
    anthropic = {
      enable = mkEnableOption "Anthropic Claude provider";

      apiKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Anthropic API key.";
      };

      model = mkOption {
        type = types.str;
        default = "claude-sonnet-4-20250514";
        description = "Anthropic model to use.";
      };

      oauthCredentialFile = mkOption {
        type = types.path;
        default = "/var/lib/loom-server/anthropic-credentials.json";
        description = "Path to OAuth credential store JSON file for Claude Max accounts.";
      };

      oauthEnabled = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable OAuth pool mode for Claude Max subscriptions.
          When enabled, accounts are managed via the admin web UI.
          Mutually exclusive with apiKeyFile.
        '';
      };

      refreshIntervalSecs = mkOption {
        type = types.int;
        default = 300;
        description = "Interval in seconds between proactive token refresh checks.";
      };

      refreshThresholdSecs = mkOption {
        type = types.int;
        default = 900;
        description = "Refresh tokens when they expire within this many seconds.";
      };
    };

    openai = {
      enable = mkEnableOption "OpenAI provider";

      apiKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing OpenAI API key.";
      };

      model = mkOption {
        type = types.str;
        default = "gpt-4o";
        description = "OpenAI model to use.";
      };

      organization = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "OpenAI organization ID.";
      };
    };

    vertex = {
      enable = mkEnableOption "Google Vertex AI provider";

      projectId = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Google Cloud project ID.";
      };

      location = mkOption {
        type = types.str;
        default = "us-central1";
        description = "Google Cloud region.";
      };

      credentialsFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to Google Cloud service account credentials JSON file.";
      };
    };

    zai = {
      enable = mkEnableOption "Z.ai (智谱AI/ZhipuAI) provider";

      apiKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Z.ai API key.";
      };

      model = mkOption {
        type = types.str;
        default = "glm-4.7";
        description = "Z.ai model to use.";
      };
    };

    # GitHub App Configuration (for repository integrations)
    githubApp = {
      enable = mkEnableOption "GitHub App integration";

      appIdFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing GitHub App ID.";
      };

      privateKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing GitHub App private key.";
      };

      webhookSecretFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing GitHub webhook secret.";
      };

      slug = mkOption {
        type = types.str;
        default = "loom";
        description = "GitHub App slug (appears in installation URLs).";
      };

      baseUrl = mkOption {
        type = types.str;
        default = "https://api.github.com";
        description = "GitHub API base URL (for GitHub Enterprise Server).";
      };
    };

    # GitHub OAuth Configuration (for user authentication)
    githubOAuth = {
      enable = mkEnableOption "GitHub OAuth for user authentication";

      clientIdFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing GitHub OAuth client ID.";
      };

      clientSecretFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing GitHub OAuth client secret.";
      };

      redirectUri = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          Callback URL for GitHub OAuth (e.g., https://loom.example.com/auth/github/callback).
          If not set, will be derived from baseUrl.
        '';
      };
    };

    # Google OAuth Configuration (for user authentication)
    googleOAuth = {
      enable = mkEnableOption "Google OAuth for user authentication";

      clientIdFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Google OAuth client ID.";
      };

      clientSecretFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Google OAuth client secret.";
      };

      redirectUri = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          Callback URL for Google OAuth (e.g., https://loom.example.com/auth/google/callback).
          If not set, will be derived from baseUrl.
        '';
      };
    };

    # Okta OAuth Configuration (for enterprise SSO)
    oktaOAuth = {
      enable = mkEnableOption "Okta OAuth for enterprise SSO";

      domain = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Okta domain (e.g., your-org.okta.com).";
      };

      clientIdFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Okta OAuth client ID.";
      };

      clientSecretFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Okta OAuth client secret.";
      };

      redirectUri = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          Callback URL for Okta OAuth (e.g., https://loom.example.com/auth/okta/callback).
          If not set, will be derived from baseUrl.
        '';
      };
    };

    # Google Custom Search Engine Configuration
    googleCse = {
      enable = mkEnableOption "Google Custom Search Engine";

      apiKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Google API key.";
      };

      searchEngineIdFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Google Custom Search Engine ID.";
      };
    };

    # Serper.dev Google Search API Configuration
    serper = {
      enable = mkEnableOption "Serper.dev Google Search API";

      apiKeyFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing Serper API key.";
      };
    };

    # GeoIP Configuration
    geoip = {
      enable = mkEnableOption "GeoIP lookup service using MaxMind databases";

      databasePath = mkOption {
        type = types.path;
        default = "/var/lib/GeoIP/GeoLite2-City.mmdb";
        description = "Path to the MaxMind GeoIP database file.";
      };
    };

    # SMTP Configuration
    smtp = {
      enable = mkEnableOption "SMTP email sending";

      host = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = "SMTP server hostname.";
      };

      port = mkOption {
        type = types.port;
        default = 2525;
        description = "SMTP server port.";
      };

      username = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "SMTP username for authentication.";
      };

      passwordFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = "Path to file containing SMTP password.";
      };

      fromAddress = mkOption {
        type = types.str;
        description = "Email address to send from.";
        example = "noreply@example.com";
      };

      fromName = mkOption {
        type = types.str;
        default = "Loom";
        description = "Display name for sent emails.";
      };

      useTLS = mkOption {
        type = types.bool;
        default = false;
        description = "Whether to use TLS for SMTP connection.";
      };
    };

    # Weaver Provisioner Configuration
    weaver = {
      enable = mkEnableOption "Weaver provisioner for Kubernetes-based code execution environments";

      namespace = mkOption {
        type = types.str;
        default = "loom-weavers";
        description = "Kubernetes namespace for weavers.";
      };

      kubeconfigPath = mkOption {
        type = types.path;
        default = "/etc/rancher/k3s/k3s.yaml";
        description = ''
          Path to kubeconfig file.
          Note: This file must exist and be readable by the loom-server user.
          When using k3s, ensure the loom-server user is in the loom-k3s group.
        '';
      };

      cleanupIntervalSecs = mkOption {
        type = types.int;
        default = 1800;
        description = "Cleanup interval in seconds for expired weavers.";
      };

      defaultTtlHours = mkOption {
        type = types.int;
        default = 4;
        description = "Default TTL in hours for weavers.";
      };

      maxTtlHours = mkOption {
        type = types.int;
        default = 48;
        description = "Maximum TTL in hours for weavers.";
      };

      maxConcurrent = mkOption {
        type = types.int;
        default = 64;
        description = "Maximum number of concurrent weavers.";
      };

      readyTimeoutSecs = mkOption {
        type = types.int;
        default = 60;
        description = "Timeout in seconds waiting for weaver to become ready.";
      };

      webhooks = mkOption {
        type = types.str;
        default = "[]";
        description = "JSON string of webhook configurations.";
      };

      imagePullSecrets = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "List of Kubernetes secret names for pulling private container images.";
        example = [ "ghcr-secret" ];
      };

      audit = {
        enable = mkOption {
          type = types.bool;
          default = true;
          description = "Enable eBPF audit sidecar for weavers. Enabled by default when weaver is enabled.";
        };

        image = mkOption {
          type = types.str;
          default = "ghcr.io/ghuntley/loom-audit-sidecar:latest";
          description = "Container image for the audit sidecar.";
        };

        batchIntervalMs = mkOption {
          type = types.int;
          default = 100;
          description = "Event batch interval in milliseconds.";
        };

        bufferMaxBytes = mkOption {
          type = types.int;
          default = 268435456;  # 256 MB
          description = "Maximum local buffer size in bytes.";
        };

        metricsPort = mkOption {
          type = types.port;
          default = 9090;
          description = "Prometheus metrics port.";
        };

        healthPort = mkOption {
          type = types.port;
          default = 9091;
          description = "Health endpoint port.";
        };
      };
    };

    jobs = {
      alertEnabled = mkOption {
        type = types.bool;
        default = false;
        description = "Enable email alerts for job failures";
      };

      alertRecipients = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Email recipients for job failure alerts";
      };

      historyRetentionDays = mkOption {
        type = types.int;
        default = 90;
        description = "Number of days to retain job run history";
      };

      sessionCleanupIntervalSecs = mkOption {
        type = types.int;
        default = 3600;
        description = "Interval in seconds between session cleanup runs";
      };

      oauthStateCleanupIntervalSecs = mkOption {
        type = types.int;
        default = 900;
        description = "Interval in seconds between OAuth state cleanup runs";
      };

      scmMaintenanceEnabled = mkOption {
        type = types.bool;
        default = true;
        description = "Enable periodic git maintenance (gc, prune, repack, fsck) on SCM repositories";
      };

      scmMaintenanceIntervalSecs = mkOption {
        type = types.int;
        default = 86400;
        description = "Interval in seconds between SCM git maintenance runs (default: 24 hours)";
      };

      scmMaintenanceStaggerMs = mkOption {
        type = types.int;
        default = 100;
        description = "Delay in milliseconds between processing each repository during maintenance";
      };
    };

    # SCIM (System for Cross-domain Identity Management)
    # Enables automatic user provisioning from IdPs like Okta, Azure AD, OneLogin
    scim = {
      enable = mkEnableOption "SCIM 2.0 provisioning for enterprise IdP integration";

      tokenFile = mkOption {
        type = types.nullOr types.path;
        default = null;
        description = ''
          Path to file containing the SCIM bearer token.
          This token is shared with your IdP (e.g., Okta) for authentication.
          Generate with: openssl rand -base64 32
        '';
      };

      orgId = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          UUID of the Loom organization to provision users into.
          All SCIM-provisioned users will be added to this organization.
        '';
      };
    };

    extraEnvironment = mkOption {
      type = types.attrsOf types.str;
      default = { };
      description = "Extra environment variables to pass to the server.";
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.anthropic.enable -> (cfg.anthropic.oauthEnabled || cfg.anthropic.apiKeyFile != null);
        message = "services.loom-server.anthropic: either oauthEnabled or apiKeyFile must be set when Anthropic is enabled.";
      }
      {
        assertion = !(cfg.anthropic.enable && cfg.anthropic.oauthEnabled && cfg.anthropic.apiKeyFile != null);
        message = "services.loom-server.anthropic: oauthEnabled and apiKeyFile are mutually exclusive.";
      }
      {
        assertion = cfg.openai.enable -> cfg.openai.apiKeyFile != null;
        message = "services.loom-server.openai.apiKeyFile must be set when OpenAI is enabled.";
      }
      {
        assertion = cfg.vertex.enable -> (cfg.vertex.projectId != null && cfg.vertex.credentialsFile != null);
        message = "services.loom-server.vertex.projectId and credentialsFile must be set when Vertex AI is enabled.";
      }
      {
        assertion = cfg.githubApp.enable -> (cfg.githubApp.appIdFile != null && cfg.githubApp.privateKeyFile != null);
        message = "services.loom-server.githubApp.appIdFile and privateKeyFile must be set when GitHub App is enabled.";
      }
      {
        assertion = cfg.githubOAuth.enable -> (cfg.githubOAuth.clientIdFile != null && cfg.githubOAuth.clientSecretFile != null);
        message = "services.loom-server.githubOAuth.clientIdFile and clientSecretFile must be set when GitHub OAuth is enabled.";
      }
      {
        assertion = cfg.googleOAuth.enable -> (cfg.googleOAuth.clientIdFile != null && cfg.googleOAuth.clientSecretFile != null);
        message = "services.loom-server.googleOAuth.clientIdFile and clientSecretFile must be set when Google OAuth is enabled.";
      }
      {
        assertion = cfg.oktaOAuth.enable -> (cfg.oktaOAuth.domain != null && cfg.oktaOAuth.clientIdFile != null && cfg.oktaOAuth.clientSecretFile != null);
        message = "services.loom-server.oktaOAuth.domain, clientIdFile and clientSecretFile must be set when Okta OAuth is enabled.";
      }
      {
        assertion = cfg.googleCse.enable -> (cfg.googleCse.apiKeyFile != null && cfg.googleCse.searchEngineIdFile != null);
        message = "services.loom-server.googleCse.apiKeyFile and searchEngineIdFile must be set when Google CSE is enabled.";
      }
      {
        assertion = cfg.serper.enable -> cfg.serper.apiKeyFile != null;
        message = "services.loom-server.serper.apiKeyFile must be set when Serper is enabled.";
      }
      {
        assertion = cfg.secrets.enable -> cfg.secrets.masterKeyFile != null;
        message = "services.loom-server.secrets.masterKeyFile must be set when secrets system is enabled.";
      }
      {
        assertion = cfg.scim.enable -> (cfg.scim.tokenFile != null && cfg.scim.orgId != null);
        message = "services.loom-server.scim.tokenFile and orgId must be set when SCIM is enabled.";
      }
    ];

    users.users.loom-server = {
      isSystemUser = true;
      group = "loom-server";
      home = "/var/lib/loom-server";
      createHome = true;
      description = "Loom server service user";
      extraGroups = mkIf cfg.weaver.enable [ "loom-k3s" ];
    };

    users.groups.loom-server = { };

    # Create SCM repos directory
    systemd.tmpfiles.rules = [
      "d /var/lib/loom 0755 root root -"
      "d /var/lib/loom/repos 0755 loom-server loom-server -"
    ];

    systemd.services.loom-server = {
      description = "Loom Server - HTTP server for Loom AI coding assistant";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      environment = mkMerge [
        {
          LOOM_SERVER_HOST = cfg.host;
          LOOM_SERVER_PORT = toString cfg.port;
          LOOM_SERVER_DATABASE_URL = "sqlite:${cfg.databasePath}";
          LOOM_SERVER_DEFAULT_LOCALE = cfg.defaultLocale;
          RUST_LOG = cfg.logLevel;
        }
        (mkIf (cfg.baseUrl != null) {
          LOOM_SERVER_BASE_URL = cfg.baseUrl;
        })
        (mkIf cfg.signupsDisabled {
          LOOM_SERVER_SIGNUPS_DISABLED = "true";
        })
        (mkIf cfg.anthropic.enable {
          LOOM_SERVER_ANTHROPIC_MODEL = cfg.anthropic.model;
        })
        (mkIf (cfg.anthropic.enable && cfg.anthropic.oauthEnabled) {
          LOOM_SERVER_ANTHROPIC_OAUTH_ENABLED = "true";
          LOOM_SERVER_ANTHROPIC_OAUTH_CREDENTIAL_FILE = cfg.anthropic.oauthCredentialFile;
          LOOM_SERVER_ANTHROPIC_REFRESH_INTERVAL_SECS = toString cfg.anthropic.refreshIntervalSecs;
          LOOM_SERVER_ANTHROPIC_REFRESH_THRESHOLD_SECS = toString cfg.anthropic.refreshThresholdSecs;
        })
        (mkIf cfg.openai.enable {
          LOOM_SERVER_OPENAI_MODEL = cfg.openai.model;
        })
        (mkIf (cfg.openai.enable && cfg.openai.organization != null) {
          LOOM_SERVER_OPENAI_ORG = cfg.openai.organization;
        })
        (mkIf cfg.vertex.enable {
          LOOM_SERVER_VERTEX_PROJECT_ID = cfg.vertex.projectId;
          LOOM_SERVER_VERTEX_LOCATION = cfg.vertex.location;
        })
        (mkIf cfg.githubApp.enable {
          LOOM_SERVER_GITHUB_APP_SLUG = cfg.githubApp.slug;
          LOOM_SERVER_GITHUB_APP_BASE_URL = cfg.githubApp.baseUrl;
        })
        (mkIf (cfg.githubOAuth.enable && cfg.githubOAuth.redirectUri != null) {
          LOOM_SERVER_GITHUB_REDIRECT_URI = cfg.githubOAuth.redirectUri;
        })
        (mkIf (cfg.googleOAuth.enable && cfg.googleOAuth.redirectUri != null) {
          LOOM_SERVER_GOOGLE_REDIRECT_URI = cfg.googleOAuth.redirectUri;
        })
        (mkIf cfg.oktaOAuth.enable {
          LOOM_SERVER_OKTA_DOMAIN = cfg.oktaOAuth.domain;
        })
        (mkIf (cfg.oktaOAuth.enable && cfg.oktaOAuth.redirectUri != null) {
          LOOM_SERVER_OKTA_REDIRECT_URI = cfg.oktaOAuth.redirectUri;
        })
        (mkIf (cfg.binDir != null) {
          LOOM_SERVER_BIN_DIR = toString cfg.binDir;
        })
        # Use binariesPackage if binDir not explicitly set but platforms are configured
        (mkIf (cfg.binDir == null && cfg.binPlatforms.linux-x86_64) {
          LOOM_SERVER_BIN_DIR = toString binariesPackage;
        })
        (mkIf cfg.weaver.enable {
          LOOM_SERVER_WEAVER_ENABLED = "true";
          LOOM_SERVER_WEAVER_K8S_NAMESPACE = cfg.weaver.namespace;
          LOOM_SERVER_WEAVER_CLEANUP_INTERVAL_SECS = toString cfg.weaver.cleanupIntervalSecs;
          LOOM_SERVER_WEAVER_DEFAULT_TTL_HOURS = toString cfg.weaver.defaultTtlHours;
          LOOM_SERVER_WEAVER_MAX_TTL_HOURS = toString cfg.weaver.maxTtlHours;
          LOOM_SERVER_WEAVER_MAX_CONCURRENT = toString cfg.weaver.maxConcurrent;
          LOOM_SERVER_WEAVER_READY_TIMEOUT_SECS = toString cfg.weaver.readyTimeoutSecs;
          LOOM_SERVER_WEAVER_WEBHOOKS = cfg.weaver.webhooks;
          LOOM_SERVER_WEAVER_IMAGE_PULL_SECRETS = lib.concatStringsSep "," cfg.weaver.imagePullSecrets;
          LOOM_SERVER_WEAVER_AUDIT_ENABLED = if cfg.weaver.audit.enable then "true" else "false";
          LOOM_SERVER_WEAVER_AUDIT_IMAGE = cfg.weaver.audit.image;
          LOOM_SERVER_WEAVER_AUDIT_BATCH_INTERVAL_MS = toString cfg.weaver.audit.batchIntervalMs;
          LOOM_SERVER_WEAVER_AUDIT_BUFFER_MAX_BYTES = toString cfg.weaver.audit.bufferMaxBytes;
          LOOM_SERVER_WEAVER_AUDIT_METRICS_PORT = toString cfg.weaver.audit.metricsPort;
          LOOM_SERVER_WEAVER_AUDIT_HEALTH_PORT = toString cfg.weaver.audit.healthPort;
          KUBECONFIG = toString cfg.weaver.kubeconfigPath;
        })
        (mkIf cfg.secrets.enable {
          LOOM_SECRETS_SVID_TTL_SECONDS = toString cfg.secrets.svidTtlSeconds;
          LOOM_SECRETS_VERIFY_POD_EXISTS = if cfg.secrets.verifyPodExists then "true" else "false";
        })
        (mkIf cfg.geoip.enable {
          LOOM_SERVER_GEOIP_DATABASE_PATH = toString cfg.geoip.databasePath;
        })
        (mkIf cfg.smtp.enable {
          LOOM_SERVER_SMTP_HOST = cfg.smtp.host;
          LOOM_SERVER_SMTP_PORT = toString cfg.smtp.port;
          LOOM_SERVER_SMTP_FROM_ADDRESS = cfg.smtp.fromAddress;
          LOOM_SERVER_SMTP_FROM_NAME = cfg.smtp.fromName;
          LOOM_SERVER_SMTP_USE_TLS = if cfg.smtp.useTLS then "true" else "false";
        })
        (mkIf (cfg.smtp.enable && cfg.smtp.username != null) {
          LOOM_SERVER_SMTP_USERNAME = cfg.smtp.username;
        })
        {
          LOOM_SERVER_JOB_ALERT_ENABLED = if cfg.jobs.alertEnabled then "true" else "false";
          LOOM_SERVER_JOB_ALERT_RECIPIENTS = lib.concatStringsSep "," cfg.jobs.alertRecipients;
          LOOM_SERVER_JOB_HISTORY_RETENTION_DAYS = toString cfg.jobs.historyRetentionDays;
          LOOM_SERVER_SESSION_CLEANUP_INTERVAL_SECS = toString cfg.jobs.sessionCleanupIntervalSecs;
          LOOM_SERVER_OAUTH_STATE_CLEANUP_INTERVAL_SECS = toString cfg.jobs.oauthStateCleanupIntervalSecs;
          LOOM_SERVER_SCM_MAINTENANCE_ENABLED = if cfg.jobs.scmMaintenanceEnabled then "true" else "false";
          LOOM_SERVER_SCM_MAINTENANCE_INTERVAL_SECS = toString cfg.jobs.scmMaintenanceIntervalSecs;
          LOOM_SERVER_SCM_MAINTENANCE_STAGGER_MS = toString cfg.jobs.scmMaintenanceStaggerMs;
          LOOM_SERVER_DATA_DIR = "/var/lib/loom";
        }
        (mkIf (cfg.docsIndexPath != null) {
          LOOM_SERVER_DOCS_INDEX = toString cfg.docsIndexPath;
        })
        (mkIf cfg.scim.enable {
          LOOM_SERVER_SCIM_ENABLED = "true";
          LOOM_SERVER_SCIM_ORG_ID = cfg.scim.orgId;
        })
        cfg.extraEnvironment
      ];

      script = let
        loadSecret = file: envVar: optionalString (file != null) ''
          export ${envVar}="$(cat ${file})"
        '';
      in ''
        # LLM Provider Secrets
        ${loadSecret cfg.anthropic.apiKeyFile "LOOM_SERVER_ANTHROPIC_API_KEY"}
        ${loadSecret cfg.openai.apiKeyFile "LOOM_SERVER_OPENAI_API_KEY"}
        ${loadSecret cfg.vertex.credentialsFile "GOOGLE_APPLICATION_CREDENTIALS"}

        # GitHub App Secrets (repository integrations)
        ${loadSecret cfg.githubApp.appIdFile "LOOM_SERVER_GITHUB_APP_ID"}
        ${loadSecret cfg.githubApp.privateKeyFile "LOOM_SERVER_GITHUB_APP_PRIVATE_KEY"}
        ${loadSecret cfg.githubApp.webhookSecretFile "LOOM_SERVER_GITHUB_APP_WEBHOOK_SECRET"}

        # GitHub OAuth Secrets (user authentication)
        ${loadSecret cfg.githubOAuth.clientIdFile "LOOM_SERVER_GITHUB_CLIENT_ID"}
        ${loadSecret cfg.githubOAuth.clientSecretFile "LOOM_SERVER_GITHUB_CLIENT_SECRET"}

        # Google OAuth Secrets (user authentication)
        ${loadSecret cfg.googleOAuth.clientIdFile "LOOM_SERVER_GOOGLE_CLIENT_ID"}
        ${loadSecret cfg.googleOAuth.clientSecretFile "LOOM_SERVER_GOOGLE_CLIENT_SECRET"}

        # Okta OAuth Secrets (enterprise SSO)
        ${loadSecret cfg.oktaOAuth.clientIdFile "LOOM_SERVER_OKTA_CLIENT_ID"}
        ${loadSecret cfg.oktaOAuth.clientSecretFile "LOOM_SERVER_OKTA_CLIENT_SECRET"}

        # Google CSE Secrets
        ${loadSecret cfg.googleCse.apiKeyFile "LOOM_SERVER_GOOGLE_CSE_API_KEY"}
        ${loadSecret cfg.googleCse.searchEngineIdFile "LOOM_SERVER_GOOGLE_CSE_SEARCH_ENGINE_ID"}

        # Serper Secrets
        ${loadSecret cfg.serper.apiKeyFile "LOOM_SERVER_SERPER_API_KEY"}

        # SMTP Secrets
        ${loadSecret cfg.smtp.passwordFile "LOOM_SERVER_SMTP_PASSWORD"}

        # SCIM Secrets
        ${loadSecret cfg.scim.tokenFile "LOOM_SERVER_SCIM_TOKEN"}

        # Weaver Secrets System (pass file paths, not contents)
        ${optionalString (cfg.secrets.masterKeyFile != null) ''
          export LOOM_SECRETS_MASTER_KEY_FILE="${cfg.secrets.masterKeyFile}"
        ''}
        ${optionalString (cfg.secrets.svidSigningKeyFile != null) ''
          export LOOM_SECRETS_SVID_SIGNING_KEY_FILE="${cfg.secrets.svidSigningKeyFile}"
        ''}

        exec ${cfg.package}/bin/loom-server
      '';

      serviceConfig = {
        Type = "simple";
        User = "loom-server";
        Group = "loom-server";
        WorkingDirectory = "/var/lib/loom-server";
        StateDirectory = "loom-server";
        RuntimeDirectory = "loom-server";

        # Wait for port to be free before starting (prevents race during restarts)
        ExecStartPre = "${pkgs.bash}/bin/bash -c 'for i in {1..10}; do ${pkgs.iproute2}/bin/ss -tlnp | grep -q \":${toString cfg.port} \" || exit 0; sleep 0.5; done; exit 1'";

        # Security hardening
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = [ "/var/lib/loom" ];
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        MemoryDenyWriteExecute = true;
        LockPersonality = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [ "@system-service" "~@privileged" "~@resources" ];

        # Restart policy
        Restart = "on-failure";
        RestartSec = "5s";

        StandardOutput = "journal";
        StandardError = "journal";
      };
    };
  };
}
