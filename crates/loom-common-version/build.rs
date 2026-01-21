// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

fn main() -> shadow_rs::SdResult<()> {
	let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
	let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
	println!("cargo:rustc-env=LOOM_PLATFORM={os}-{arch}");
	shadow_rs::new()
}
