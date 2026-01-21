// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Symbol artifact types for crash symbolication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::CrashError;
use crate::project::ProjectId;
use crate::{OrgId, UserId};

/// Unique identifier for a symbol artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SymbolArtifactId(pub Uuid);

impl SymbolArtifactId {
	pub fn new() -> Self {
		Self(Uuid::now_v7())
	}
}

impl Default for SymbolArtifactId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for SymbolArtifactId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for SymbolArtifactId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Uploaded debug artifact for symbolication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SymbolArtifact {
	pub id: SymbolArtifactId,
	pub org_id: OrgId,
	pub project_id: ProjectId,

	/// Version this applies to
	pub release: String,
	/// Distribution variant
	pub dist: Option<String>,
	pub artifact_type: ArtifactType,
	/// e.g., "main.js.map", "app.min.js"
	pub name: String,

	/// Blob content (not serialized for API responses)
	#[serde(skip)]
	pub data: Vec<u8>,
	pub size_bytes: u64,
	/// For deduplication
	pub sha256: String,

	/// //# sourceMappingURL value
	pub source_map_url: Option<String>,
	/// Whether sourcesContent is embedded
	pub sources_content: bool,

	pub uploaded_at: DateTime<Utc>,
	pub uploaded_by: UserId,
	pub last_accessed_at: Option<DateTime<Utc>>,
}

/// Type of debug artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
	/// .map files
	SourceMap,
	/// Minified .js files (for URL matching)
	MinifiedSource,
	/// Future: Rust debug symbols
	RustDebugInfo,
}

impl fmt::Display for ArtifactType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::SourceMap => write!(f, "source_map"),
			Self::MinifiedSource => write!(f, "minified_source"),
			Self::RustDebugInfo => write!(f, "rust_debug_info"),
		}
	}
}

impl FromStr for ArtifactType {
	type Err = CrashError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"source_map" => Ok(Self::SourceMap),
			"minified_source" => Ok(Self::MinifiedSource),
			"rust_debug_info" => Ok(Self::RustDebugInfo),
			_ => Err(CrashError::InvalidArtifactType(s.to_string())),
		}
	}
}

/// Metadata about a parsed source map.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceMapMetadata {
	/// Should be 3
	pub version: u32,
	pub file: Option<String>,
	pub source_root: Option<String>,
	pub sources: Vec<String>,
	pub names: Vec<String>,
	pub has_sources_content: bool,
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn symbol_artifact_id_roundtrip(uuid_bytes in any::<[u8; 16]>()) {
			let uuid = Uuid::from_bytes(uuid_bytes);
			let id = SymbolArtifactId(uuid);
			let s = id.to_string();
			let parsed: SymbolArtifactId = s.parse().unwrap();
			prop_assert_eq!(id, parsed);
		}

		#[test]
		fn artifact_type_roundtrip(artifact_type in prop_oneof![
			Just(ArtifactType::SourceMap),
			Just(ArtifactType::MinifiedSource),
			Just(ArtifactType::RustDebugInfo),
		]) {
			let s = artifact_type.to_string();
			let parsed: ArtifactType = s.parse().unwrap();
			prop_assert_eq!(artifact_type, parsed);
		}
	}
}
