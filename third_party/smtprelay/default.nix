# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

{ lib
, buildGoModule
, fetchFromGitHub
}:

buildGoModule rec {
  pname = "smtprelay";
  # Note: v2.3.0 requires Go 1.25.3+ which isn't available in nixpkgs yet
  version = "2.2.3";

  src = fetchFromGitHub {
    owner = "grafana";
    repo = "smtprelay";
    rev = "v${version}";
    hash = "sha256-phQsVH6kEvgyXvaBo7FmyxU8QOKnZKC5XQZhWhAFxE8=";
  };

  vendorHash = "sha256-QHNlGM/gRbpBdBVzRxIEHn0U7ixWp6SKAHYxV5T7X4M=";

  ldflags = [
    "-s" 
    "-w"
    "-X github.com/grafana/smtprelay/v2/main.version=v${version}"
    "-X github.com/grafana/smtprelay/v2/main.branch=nix-build"
    "-X github.com/grafana/smtprelay/v2/main.revision=${src.rev}"
  ];

  # Skip tests that may require network access or external dependencies
  doCheck = false;

  meta = with lib; {
    description = "Simple Golang based SMTP relay/proxy server (Grafana fork)";
    longDescription = ''
      Simple SMTP relay/proxy server that accepts mail via SMTP and forwards 
      it directly to another SMTP server. Supports multiple SMTP protocols 
      (SMTPS/TLS, STARTTLS, unencrypted) with authentication and access controls.
      This is the Grafana Labs maintained fork with additional features and improvements.
    '';
    homepage = "https://github.com/grafana/smtprelay";
    license = licenses.mit;
    maintainers = with maintainers; [ ];
    mainProgram = "smtprelay";
  };
}