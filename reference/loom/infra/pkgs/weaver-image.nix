# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Nix expression for building loom-weaver Docker image
# Uses nixpkgs.dockerTools for reproducible image builds
#
# The weaver image provides an ephemeral environment for running loom REPL
# sessions in isolated Kubernetes pods. It includes:
# - loom CLI binary
# - git for repository cloning
# - Development tools (gh, btop, tmux, jq)
# - Entrypoint script for repo cloning and REPL startup

{ lib
, dockerTools
, buildEnv
, writeShellScriptBin
, writeTextDir
, symlinkJoin
, loom-cli
, cacert
, git
, curl
, gh
, btop
, tmux
, jq
, dive
, devenv
, direnv
, starship
, neovim
, coreutils
, bashInteractive
}:

let
  # Entrypoint script for weaver pods
  entrypoint = writeShellScriptBin "entrypoint" ''
    #!/bin/bash
    # Weaver pod entrypoint script
    # Clones a git repository if specified, then starts loom REPL in tmux
    #
    # We use tmux to provide a persistent PTY for the loom REPL.
    # Without tmux, the container's stdin is not connected to anything,
    # causing the REPL to receive immediate EOF and exit.
    # With tmux, the REPL has a proper PTY and waits for input.
    # When clients attach via K8s attach API, they connect to tmux.

    set -e

    WORKSPACE="/workspace"

    # Clone repository if LOOM_REPO is set
    if [ -n "$LOOM_REPO" ]; then
      echo "Cloning $LOOM_REPO..."
      
      if [ -n "$LOOM_BRANCH" ]; then
        ${git}/bin/git clone --branch "$LOOM_BRANCH" --single-branch "$LOOM_REPO" "$WORKSPACE"
      else
        ${git}/bin/git clone "$LOOM_REPO" "$WORKSPACE"
      fi
      
      cd "$WORKSPACE"
      echo "Cloning complete."
      echo ""
    else
      mkdir -p "$WORKSPACE"
      cd "$WORKSPACE"
    fi

    # Start loom REPL inside tmux session
    # -A: attach to existing session or create new one
    # -s loom: name the session "loom"
    # The tmux session provides a PTY so loom doesn't get EOF
    exec ${tmux}/bin/tmux new-session -A -s loom "${loom-cli}/bin/loom"
  '';

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

  # Create bashrc with starship init
  bashrcFile = writeTextDir "home/loom/.bashrc" ''
    # Loom Weaver bashrc
    
    # Initialize starship prompt
    eval "$(${starship}/bin/starship init bash)"
    
    # Initialize direnv
    eval "$(${direnv}/bin/direnv hook bash)"
    
    # Set PATH
    export PATH="/bin:$PATH"
    
    # Editor aliases
    alias vi='nvim'
    alias vim='nvim'
    export EDITOR=nvim
    export VISUAL=nvim
    
    # Welcome message
    echo "Welcome to Loom Weaver"
    echo ""
  '';

  # Merge passwd/group with cacert's etc
  etcFiles = symlinkJoin {
    name = "etc-merged";
    paths = [ passwdFile groupFile ];
  };

  # Home directory files
  homeFiles = symlinkJoin {
    name = "home-files";
    paths = [ bashrcFile ];
  };
in
dockerTools.buildImage {
  name = "loom-weaver";
  tag = "latest";

  # Copy binaries and runtime dependencies to image root
  copyToRoot = buildEnv {
    name = "weaver-root";
    paths = [
      loom-cli
      entrypoint
      cacert
      etcFiles
      homeFiles
      git
      curl
      gh
      btop
      tmux
      jq
      dive
      devenv
      direnv
      starship
      neovim
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
    mkdir -p /workspace
    mkdir -p /tmp

    # Set permissions
    chmod 1777 /tmp
    chown -R 1000:1000 /home/loom
    chmod 755 /home/loom
    chown -R 1000:1000 /workspace
    chmod 755 /workspace
  '';

  # Container configuration
  config = {
    # Run as non-root user
    User = "1000:1000";

    # Entrypoint
    Entrypoint = [ "${entrypoint}/bin/entrypoint" ];

    # Exposed ports (none needed for weaver)
    ExposedPorts = { };

    # Environment variables
    Env = [
      "RUST_LOG=info"
      "PATH=/bin"
      "HOME=/home/loom"
      "USER=loom"
      "TERM=xterm-256color"
      "SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt"
    ];

    # Working directory
    WorkingDir = "/workspace";

    # Labels
    Labels = {
      "org.opencontainers.image.title" = "Loom Weaver";
      "org.opencontainers.image.description" = "Ephemeral environment for Loom REPL sessions";
      "org.opencontainers.image.source" = "https://github.com/ghuntley/loom";
    };
  };
}
