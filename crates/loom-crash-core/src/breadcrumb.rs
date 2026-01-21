// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Breadcrumb types for crash events (events leading up to crash).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::CrashError;

/// A breadcrumb representing an event leading up to the crash.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Breadcrumb {
	pub timestamp: DateTime<Utc>,
	/// "http", "navigation", "ui", "console"
	pub category: String,
	pub message: Option<String>,
	pub level: BreadcrumbLevel,
	pub data: serde_json::Value,
}

impl Default for Breadcrumb {
	fn default() -> Self {
		Self {
			timestamp: Utc::now(),
			category: String::new(),
			message: None,
			level: BreadcrumbLevel::Info,
			data: serde_json::Value::Object(serde_json::Map::new()),
		}
	}
}

/// Severity level of a breadcrumb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BreadcrumbLevel {
	Debug,
	Info,
	Warning,
	Error,
}

impl fmt::Display for BreadcrumbLevel {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Debug => write!(f, "debug"),
			Self::Info => write!(f, "info"),
			Self::Warning => write!(f, "warning"),
			Self::Error => write!(f, "error"),
		}
	}
}

impl FromStr for BreadcrumbLevel {
	type Err = CrashError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"debug" => Ok(Self::Debug),
			"info" => Ok(Self::Info),
			"warning" => Ok(Self::Warning),
			"error" => Ok(Self::Error),
			_ => Err(CrashError::InvalidBreadcrumbLevel(s.to_string())),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn breadcrumb_level_roundtrip(level in prop_oneof![
			Just(BreadcrumbLevel::Debug),
			Just(BreadcrumbLevel::Info),
			Just(BreadcrumbLevel::Warning),
			Just(BreadcrumbLevel::Error),
		]) {
			let s = level.to_string();
			let parsed: BreadcrumbLevel = s.parse().unwrap();
			prop_assert_eq!(level, parsed);
		}
	}
}
