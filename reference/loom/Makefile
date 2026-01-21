# Loom Makefile
#
# Development tasks for the loom workspace.

GITLEAKS_REPO := https://raw.githubusercontent.com/gitleaks/gitleaks/master
GITLEAKS_VENDOR_DIR := crates/loom-redact/third_party/gitleaks

.PHONY: all build test lint format check clean help dev dev-server dev-web update-gitleaks sbom sbom-spdx sbom-cyclonedx release \
	docker-build docker-build-nix docker-run docker-web-build \
	web-install web-dev web-build web-test web-storybook web-storybook-build web-lint web-format web-check

# Help target
help:
	@echo "Loom Makefile Targets"
	@echo "===================="
	@echo ""
	@echo "Core Development (Rust):"
	@echo "  make build              - Build entire workspace"
	@echo "  make test               - Run all tests"
	@echo "  make lint               - Run clippy linter"
	@echo "  make format             - Format code with rustfmt"
	@echo "  make check-format       - Check formatting without modifying"
	@echo "  make fix                - Auto-fix clippy issues and format"
	@echo "  make check              - Full CI checks (format + lint + build + test)"
	@echo "  make dev                - Watch mode for development (build all)"
	@echo "  make dev-server         - Run loom-server with hot reload"
	@echo ""
	@echo "Web Development (loom-web):"
	@echo "  make web-install        - Install npm dependencies"
	@echo "  make web-dev            - Start Vite dev server"
	@echo "  make web-build          - Production build"
	@echo "  make web-test           - Run Vitest tests (with fast-check)"
	@echo "  make web-lint           - Lint and format check"
	@echo "  make web-format         - Format code with Prettier"
	@echo "  make web-check          - Full checks (lint + test + build)"
	@echo "  make web-storybook      - Start Storybook dev server"
	@echo "  make web-storybook-build - Build Storybook static site"
	@echo ""
	@echo "Code Quality:"
	@echo "  make sbom               - Generate SBOM (SPDX and CycloneDX)"
	@echo "  make sbom-spdx          - Generate SPDX SBOM"
	@echo "  make sbom-cyclonedx     - Generate CycloneDX SBOM"
	@echo ""
	@echo "Docker:"
	@echo "  make docker-build       - Build Docker image (loom-server + loom-web)"
	@echo "  make docker-run         - Build and run Docker container"
	@echo "  make docker-build-nix   - Build Docker image via Nix (reproducible)"
	@echo ""
	@echo "Release:"
	@echo "  make release            - Build release (build + test + SBOM)"
	@echo "  make update-gitleaks    - Update gitleaks rules from upstream"
	@echo "  make clean              - Clean build artifacts"

# Default target
all: format lint build test

# Build the entire workspace
build:
	cargo build --workspace

# Run all tests in workspace
test:
	cargo test --workspace

# Run clippy on entire workspace
lint:
	cargo clippy --workspace -- -D warnings

# Auto-fix clippy warnings and format code
fix:
	cargo clippy --workspace --fix --allow-dirty --allow-staged
	cargo fmt --all

# Format all code in workspace
format:
	cargo fmt --all

# Check formatting without modifying files
check-format:
	cargo fmt --all -- --check

# Development watch mode
dev:
	@echo "Starting development watch mode..."
	@echo "Watching for file changes and rebuilding..."
	@cargo watch -c -q -w src -w crates -x "build --all" -x "test --lib" 2>/dev/null || \
		(echo "Note: cargo-watch not installed. Install with: cargo install cargo-watch" && \
		 echo "For now, run 'cargo build' manually after changes")

# Run loom-server with hot reload
dev-server:
	@echo "Starting loom-server in dev mode with hot reload..."
	@cargo watch -c -q -w crates/loom-server -w crates/loom-core -w crates/loom-thread -w crates/loom-llm-service \
		-x "run -p loom-server" 2>/dev/null || \
		(echo "Note: cargo-watch not installed. Install with: cargo install cargo-watch" && \
		 cargo run -p loom-server)

# Run all checks (format check + lint + build + test)
check: check-format lint build test

# Update gitleaks.toml and LICENSE from upstream
update-gitleaks:
	@echo "Fetching gitleaks.toml from upstream..."
	@curl -fsSL "$(GITLEAKS_REPO)/config/gitleaks.toml" -o "$(GITLEAKS_VENDOR_DIR)/gitleaks.toml"
	@echo "Fetching LICENSE from upstream..."
	@curl -fsSL "$(GITLEAKS_REPO)/LICENSE" -o "$(GITLEAKS_VENDOR_DIR)/LICENSE"
	@echo "Updating README.md with source info..."
	@echo "# Vendored gitleaks rules" > "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "Source: https://github.com/gitleaks/gitleaks" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "Branch: master" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "Updated: $$(date -I)" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "## License" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "The gitleaks project is licensed under the MIT License. See LICENSE file." >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "## Usage" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "" >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "Run \`make update-gitleaks\` to refresh from upstream." >> "$(GITLEAKS_VENDOR_DIR)/README.md"
	@echo "Done! Updated gitleaks rules in $(GITLEAKS_VENDOR_DIR)/"

clean:
	rm -f "$(GITLEAKS_VENDOR_DIR)/gitleaks.toml"
	rm -f "$(GITLEAKS_VENDOR_DIR)/LICENSE"
	rm -f "$(GITLEAKS_VENDOR_DIR)/README.md"

# SBOM configuration
SBOM_DIR ?= target/sbom

# Generate SBOM in SPDX JSON 2.3 format (default)
sbom-spdx:
	@command -v cargo-sbom >/dev/null 2>&1 || \
		(echo "Error: cargo-sbom not found. Install with: cargo install cargo-sbom --version 0.10.0" && exit 1)
	mkdir -p "$(SBOM_DIR)"
	cargo sbom --output-format=spdx_json_2_3 > "$(SBOM_DIR)/loom.spdx.json"
	@echo "Generated SBOM: $(SBOM_DIR)/loom.spdx.json"

# Generate SBOM in CycloneDX JSON 1.4 format
sbom-cyclonedx:
	@command -v cargo-sbom >/dev/null 2>&1 || \
		(echo "Error: cargo-sbom not found. Install with: cargo install cargo-sbom --version 0.10.0" && exit 1)
	mkdir -p "$(SBOM_DIR)"
	cargo sbom --output-format=cyclone_dx_json_1_4 > "$(SBOM_DIR)/loom.cyclonedx.json"
	@echo "Generated SBOM: $(SBOM_DIR)/loom.cyclonedx.json"

# Generate both SPDX and CycloneDX SBOMs
sbom: sbom-spdx sbom-cyclonedx

# Release build: full build + test + SBOM
release: build test sbom
	@echo "Release build complete. Artifacts in target/debug/ and $(SBOM_DIR)/"

# Docker container targets
# Builds combined image with loom-server API and loom-web static assets

DOCKER_IMAGE_NAME ?= loom
DOCKER_IMAGE_TAG ?= latest

# Build loom-web production assets
docker-web-build:
	@echo "Building loom-web for production..."
	cd $(WEB_DIR) && pnpm install && pnpm build

# Build Docker image with both loom-server and loom-web
# Uses multi-stage Dockerfile for optimized image
docker-build: docker-web-build
	@echo "Building Docker image..."
	docker build -t $(DOCKER_IMAGE_NAME):$(DOCKER_IMAGE_TAG) -f docker/Dockerfile .
	@echo ""
	@echo "✓ Docker image built successfully"
	@echo "  Image: $(DOCKER_IMAGE_NAME):$(DOCKER_IMAGE_TAG)"
	@echo ""
	@echo "To run:"
	@echo "  docker run --rm -p 8080:8080 $(DOCKER_IMAGE_NAME):$(DOCKER_IMAGE_TAG)"

# Build Docker image via Nix flake (alternative, reproducible build)
docker-build-nix:
	@echo "Building loom-server Docker image via Nix..."
	@(set -e; \
	  nix --extra-experimental-features nix-command --extra-experimental-features flakes build .#loom-server-image -L --impure; \
	  echo ""; \
	  echo "✓ Docker image built successfully"; \
	  echo "  Output: ./result (OCI/Docker image tarball)"; \
	  echo ""; \
	  echo "To load into Docker:"; \
	  echo "  docker load < ./result")

# Run container locally
docker-run: docker-build
	@echo "Starting container (Ctrl+C to stop)..."
	docker run --rm -p 8080:8080 \
		-e RUST_LOG=info \
		$(DOCKER_IMAGE_NAME):$(DOCKER_IMAGE_TAG)

# =============================================================================
# Web Development (loom-web)
# =============================================================================

WEB_DIR := web/loom-web

# Install npm dependencies
web-install:
	@echo "Installing loom-web dependencies..."
	cd $(WEB_DIR) && pnpm install

# Start Vite dev server
web-dev:
	@echo "Starting loom-web dev server..."
	cd $(WEB_DIR) && pnpm dev

# Production build
web-build:
	@echo "Building loom-web for production..."
	cd $(WEB_DIR) && pnpm build

# Run Vitest tests (with fast-check property tests)
web-test:
	@echo "Running loom-web tests..."
	cd $(WEB_DIR) && pnpm test

# Lint and format check
web-lint:
	@echo "Linting loom-web..."
	cd $(WEB_DIR) && pnpm lint

# Format code with Prettier
web-format:
	@echo "Formatting loom-web..."
	cd $(WEB_DIR) && pnpm format

# Full checks (lint + test + build)
web-check: web-lint web-test web-build
	@echo "loom-web checks complete"

# Start Storybook dev server
web-storybook:
	@echo "Starting Storybook..."
	cd $(WEB_DIR) && pnpm storybook

# Build Storybook static site
web-storybook-build:
	@echo "Building Storybook..."
	cd $(WEB_DIR) && pnpm storybook:build

nixos-switch:
	sudo nixos-rebuild switch --flake .#virtualMachine


