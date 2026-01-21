// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StitchId(pub [u8; 16]);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TreeId(pub [u8; 20]);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId(pub [u8; 16]);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signature {
	pub name: String,
	pub email: String,
	pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stitch {
	pub id: StitchId,
	pub parents: Vec<StitchId>,
	pub tree_id: TreeId,
	pub description: String,
	pub author: Signature,
	pub committer: Signature,
	pub is_knotted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pin {
	pub name: String,
	pub target: StitchId,
	pub is_tracking: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TangleSide {
	Ours,
	Theirs,
	Base,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tangle {
	pub path: PathBuf,
	pub sides: Vec<TangleSide>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shuttle {
	pub stitch_id: StitchId,
	pub tree_state: TreeId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TensionEntry {
	pub operation_id: OperationId,
	pub timestamp: DateTime<Utc>,
	pub description: String,
}

impl StitchId {
	/// Create a StitchId from a hex string.
	/// The string can be abbreviated (less than 32 chars).
	pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
		let bytes = hex::decode(s)?;
		let mut arr = [0u8; 16];
		let len = bytes.len().min(16);
		arr[..len].copy_from_slice(&bytes[..len]);
		Ok(Self(arr))
	}

	/// Convert to a hex string (full 32 chars).
	pub fn to_hex(&self) -> String {
		hex::encode(self.0)
	}

	/// Convert to a short hex string (first 16 chars / 8 bytes).
	pub fn to_short_hex(&self) -> String {
		hex::encode(&self.0[..8])
	}
}

impl TreeId {
	/// Create a TreeId from a hex string.
	pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
		let bytes = hex::decode(s)?;
		let mut arr = [0u8; 20];
		let len = bytes.len().min(20);
		arr[..len].copy_from_slice(&bytes[..len]);
		Ok(Self(arr))
	}

	/// Convert to a hex string (full 40 chars).
	pub fn to_hex(&self) -> String {
		hex::encode(self.0)
	}
}

impl OperationId {
	/// Create an OperationId from a hex string.
	pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
		let bytes = hex::decode(s)?;
		let mut arr = [0u8; 16];
		let len = bytes.len().min(16);
		arr[..len].copy_from_slice(&bytes[..len]);
		Ok(Self(arr))
	}

	/// Convert to a hex string (full 32 chars).
	pub fn to_hex(&self) -> String {
		hex::encode(self.0)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn stitch_id_hex_roundtrip(bytes in prop::array::uniform16(any::<u8>())) {
			let id = StitchId(bytes);
			let hex_str = id.to_hex();
			let parsed = StitchId::from_hex(&hex_str).unwrap();
			prop_assert_eq!(id, parsed);
		}

		#[test]
		fn tree_id_hex_roundtrip(bytes in prop::array::uniform20(any::<u8>())) {
			let id = TreeId(bytes);
			let hex_str = id.to_hex();
			let parsed = TreeId::from_hex(&hex_str).unwrap();
			prop_assert_eq!(id, parsed);
		}

		#[test]
		fn operation_id_hex_roundtrip(bytes in prop::array::uniform16(any::<u8>())) {
			let id = OperationId(bytes);
			let hex_str = id.to_hex();
			let parsed = OperationId::from_hex(&hex_str).unwrap();
			prop_assert_eq!(id, parsed);
		}

		#[test]
		fn stitch_id_short_hex_is_prefix(bytes in prop::array::uniform16(any::<u8>())) {
			let id = StitchId(bytes);
			let short = id.to_short_hex();
			let full = id.to_hex();
			prop_assert!(full.starts_with(&short));
			prop_assert_eq!(short.len(), 16); // 8 bytes = 16 hex chars
		}

		#[test]
		fn stitch_id_from_abbreviated_hex(prefix_len in 2usize..=16) {
			let bytes = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
			let id = StitchId(bytes);
			let full_hex = id.to_hex();
			let abbreviated = &full_hex[..prefix_len * 2]; // each byte = 2 hex chars
			let parsed = StitchId::from_hex(abbreviated).unwrap();
			// First prefix_len bytes should match
			prop_assert_eq!(&parsed.0[..prefix_len], &id.0[..prefix_len]);
			// Rest should be zero
			for b in &parsed.0[prefix_len..] {
				prop_assert_eq!(*b, 0);
			}
		}
	}

	#[test]
	fn stitch_id_from_invalid_hex() {
		assert!(StitchId::from_hex("not_hex").is_err());
		assert!(StitchId::from_hex("zz").is_err());
	}

	#[test]
	fn stitch_id_from_empty_hex() {
		let id = StitchId::from_hex("").unwrap();
		assert_eq!(id.0, [0u8; 16]);
	}

	#[test]
	fn tangle_side_equality() {
		assert_eq!(TangleSide::Ours, TangleSide::Ours);
		assert_ne!(TangleSide::Ours, TangleSide::Theirs);
		assert_ne!(TangleSide::Theirs, TangleSide::Base);
	}
}
