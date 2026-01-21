// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

/// Errors that can occur in the feature flags system.
#[derive(Debug, Error)]
pub enum FlagsError {
	#[error("flag not found: {0}")]
	FlagNotFound(String),

	#[error("environment not found: {0}")]
	EnvironmentNotFound(String),

	#[error("strategy not found: {0}")]
	StrategyNotFound(String),

	#[error("kill switch not found: {0}")]
	KillSwitchNotFound(String),

	#[error("sdk key not found")]
	SdkKeyNotFound,

	#[error("invalid flag key: {0}")]
	InvalidFlagKey(String),

	#[error("invalid sdk key format")]
	InvalidSdkKeyFormat,

	#[error("sdk key revoked")]
	SdkKeyRevoked,

	#[error("duplicate flag key: {0}")]
	DuplicateFlagKey(String),

	#[error("duplicate environment name: {0}")]
	DuplicateEnvironmentName(String),

	#[error("duplicate kill switch key: {0}")]
	DuplicateKillSwitchKey(String),

	#[error("prerequisite flag not found: {0}")]
	PrerequisiteNotFound(String),

	#[error("circular prerequisite dependency: {0}")]
	CircularPrerequisite(String),

	#[error("variant not found: {0}")]
	VariantNotFound(String),

	#[error("default variant must exist in variants list")]
	DefaultVariantMissing,

	#[error("cannot delete environment with active SDK keys")]
	EnvironmentHasActiveKeys,

	#[error("cannot delete strategy in use by flags")]
	StrategyInUse,

	#[error("kill switch activation requires a reason")]
	ActivationReasonRequired,

	#[error("database error: {0}")]
	Database(String),

	#[error("serialization error: {0}")]
	Serialization(String),

	#[error("internal error: {0}")]
	Internal(String),
}

impl From<serde_json::Error> for FlagsError {
	fn from(err: serde_json::Error) -> Self {
		FlagsError::Serialization(err.to_string())
	}
}

pub type Result<T> = std::result::Result<T, FlagsError>;
