// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Argon2 configuration for password/token hashing.
//!
//! This module provides a centralized Argon2 instance that uses:
//! - Production-strength parameters in release builds
//! - Fast, reduced-cost parameters in tests for performance
//!
//! # Security Note
//!
//! Production parameters use Argon2id with strong defaults:
//! - Memory: 19456 KiB (~19 MiB)
//! - Iterations: 2
//! - Parallelism: 1
//!
//! Test parameters are intentionally weak and MUST NOT be used in production.

use argon2::Argon2;
#[cfg(test)]
use argon2::{Algorithm, Params, Version};

/// Returns an Argon2 instance configured appropriately for the build context.
///
/// In production (`#[cfg(not(test))]`), returns `Argon2::default()` with
/// strong security parameters.
///
/// In tests (`#[cfg(test)]`), returns an Argon2 instance with minimal
/// parameters for fast test execution.
#[inline]
pub(crate) fn argon2_instance() -> Argon2<'static> {
	#[cfg(test)]
	{
		// Fast, insecure parameters for tests ONLY.
		// Memory: 1024 KiB (1 MiB) vs ~19 MiB in production
		// Iterations: 1 vs 2 in production
		// Parallelism: 1
		let params = Params::new(
			1024, // memory_kib: 1 MiB
			1,    // iterations
			1,    // parallelism
			None, // output length = default
		)
		.expect("valid Argon2 params for tests");
		Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
	}

	#[cfg(not(test))]
	{
		// Production: use strong defaults
		// Argon2id with memory=19456 KiB, iterations=2, parallelism=1
		Argon2::default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_argon2_instance_returns_valid_hasher() {
		let argon2 = argon2_instance();
		// Just verify we can create an instance without panicking
		let _ = format!("{argon2:?}");
	}
}
