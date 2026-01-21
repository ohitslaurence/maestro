# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ lib
, dockerTools
, buildEnv
, writeShellScriptBin
, writeTextDir
, symlinkJoin
, runCommand
, loom-audit-sidecar
, loom-weaver-ebpf
, cacert
, coreutils
, bashInteractive
}:

let
  passwdFile = writeTextDir "etc/passwd" ''
    root:x:0:0:root:/root:/bin/bash
  '';

  groupFile = writeTextDir "etc/group" ''
    root:x:0:
  '';

  etcFiles = symlinkJoin {
    name = "etc-merged";
    paths = [ passwdFile groupFile ];
  };

  ebpfFiles = runCommand "ebpf-files" {} ''
    mkdir -p $out/opt/loom/ebpf
    cp ${loom-weaver-ebpf}/lib/ebpf/loom-weaver-ebpf $out/opt/loom/ebpf/loom-weaver-ebpf
    
    # Compute and store hash for integrity verification
    sha256sum $out/opt/loom/ebpf/loom-weaver-ebpf | cut -d' ' -f1 > $out/opt/loom/ebpf/loom-weaver-ebpf.sha256
  '';
in
dockerTools.buildImage {
  name = "loom-audit-sidecar";
  tag = "latest";

  copyToRoot = buildEnv {
    name = "audit-sidecar-root";
    paths = [
      loom-audit-sidecar
      cacert
      etcFiles
      coreutils
      bashInteractive
      ebpfFiles
    ];
    pathsToLink = [ "/bin" "/etc" "/opt" ];
  };

  runAsRoot = ''
    #!${bashInteractive}/bin/bash
    mkdir -p /tmp
    chmod 1777 /tmp
  '';

  config = {
    User = "0:0";
    Entrypoint = [ "${loom-audit-sidecar}/bin/loom-audit-sidecar" ];
    ExposedPorts = {
      "9090/tcp" = {};  # Metrics
      "9091/tcp" = {};  # Health
    };
    Env = [
      "RUST_LOG=info"
      "SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt"
    ];
    WorkingDir = "/";
    Labels = {
      "org.opencontainers.image.title" = "Loom Audit Sidecar";
      "org.opencontainers.image.description" = "eBPF-based syscall auditing for weaver pods";
      "org.opencontainers.image.source" = "https://github.com/ghuntley/loom";
    };
  };
}
