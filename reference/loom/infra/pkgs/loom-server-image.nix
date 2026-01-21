# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Nix expression for building loom-server Docker image
# Uses nixpkgs.dockerTools for reproducible image builds
#
# The server image provides the Loom API backend including:
# - loom-server binary
# - CA certificates for HTTPS
# - SQLite for database storage

{ lib
, dockerTools
, buildEnv
, writeTextDir
, symlinkJoin
, loom-server
, loom-server-binaries
, cacert
, coreutils
, bashInteractive
}:

let
  # Create passwd file with loom user
  passwdFile = writeTextDir "etc/passwd" ''
    root:x:0:0:root:/root:/bin/bash
    nobody:x:65534:65534:Nobody:/:/sbin/nologin
    loom:x:1000:1000:loom:/home/loom:/bin/bash
  '';

  # Create group file with loom group
  groupFile = writeTextDir "etc/group" ''
    root:x:0:
    nobody:x:65534:
    loom:x:1000:
  '';

  # Merge passwd/group files
  etcFiles = symlinkJoin {
    name = "etc-merged";
    paths = [ passwdFile groupFile ];
  };

  # CLI binaries directory for self-update distribution
  binDir = symlinkJoin {
    name = "loom-bin-dir";
    paths = [ loom-server-binaries ];
  };
in
dockerTools.buildImage {
  name = "loom-server";
  tag = "latest";

  # Copy binaries and runtime dependencies to image root
  copyToRoot = buildEnv {
    name = "server-root";
    paths = [
      loom-server
      cacert
      etcFiles
      coreutils
      bashInteractive
    ];
    pathsToLink = [ "/bin" "/etc" "/share" ];
  };

  # Additional configuration
  runAsRoot = ''
    #!${bashInteractive}/bin/bash
    # Create directory structure with correct ownership for loom user (1000:1000)
    mkdir -p /home/loom
    mkdir -p /var/lib/loom-server
    mkdir -p /var/lib/loom-server/bin
    mkdir -p /tmp

    # Copy CLI binaries for distribution
    cp -r ${binDir}/* /var/lib/loom-server/bin/

    # Set permissions
    chmod 1777 /tmp
    chown -R 1000:1000 /home/loom
    chmod 755 /home/loom
    chown -R 1000:1000 /var/lib/loom-server
    chmod 755 /var/lib/loom-server
  '';

  # Container configuration
  config = {
    # Run as non-root user
    User = "1000:1000";

    # Default command: run loom-server
    Cmd = [ "${loom-server}/bin/loom-server" ];

    # Exposed ports
    ExposedPorts = {
      "8080/tcp" = { };
    };

    # Environment variables
    Env = [
      "RUST_LOG=info"
      "PATH=/bin"
      "HOME=/home/loom"
      "USER=loom"
      "SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt"
      "LOOM_SERVER_DB_PATH=/var/lib/loom-server/loom.db"
      "LOOM_SERVER_BIN_DIR=/var/lib/loom-server/bin"
    ];

    # Working directory
    WorkingDir = "/var/lib/loom-server";

    # Labels
    Labels = {
      "org.opencontainers.image.title" = "Loom Server";
      "org.opencontainers.image.description" = "Loom API backend server";
      "org.opencontainers.image.source" = "https://github.com/ghuntley/loom";
    };
  };
}
