# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# eBPF Build Makefile
#
# Prerequisites:
#   rustup component add rust-src --toolchain nightly
#   rustup component add llvm-tools --toolchain nightly

.PHONY: build build-release clean setup

# Build eBPF program (debug)
# Note: rust-toolchain.toml specifies nightly, so +nightly is optional
build:
	cargo build -Z build-std=core --target bpfel-unknown-none

# Build eBPF program (release)
build-release:
	cargo build -Z build-std=core --target bpfel-unknown-none --release

# Clean build artifacts
clean:
	cargo clean

# Setup toolchain components
setup:
	rustup component add rust-src --toolchain nightly
	rustup component add llvm-tools --toolchain nightly
