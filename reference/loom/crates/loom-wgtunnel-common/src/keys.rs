// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use loom_common_secret::Secret;
use rand::rngs::OsRng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

#[derive(Error, Debug)]
pub enum KeyError {
	#[error("invalid key length: expected 32 bytes, got {0}")]
	InvalidLength(usize),

	#[error("invalid base64 encoding: {0}")]
	InvalidBase64(#[from] base64::DecodeError),

	#[error("invalid hex encoding: {0}")]
	InvalidHex(#[from] hex::FromHexError),
}

pub type Result<T> = std::result::Result<T, KeyError>;

#[derive(Clone, Zeroize)]
#[zeroize(drop)]
pub struct WgPrivateKey {
	bytes: [u8; 32],
}

impl WgPrivateKey {
	pub fn generate() -> Self {
		let secret = StaticSecret::random_from_rng(OsRng);
		Self {
			bytes: secret.to_bytes(),
		}
	}

	pub fn from_bytes(bytes: [u8; 32]) -> Self {
		Self { bytes }
	}

	pub fn from_base64(s: &str) -> Result<Self> {
		let bytes = STANDARD_NO_PAD.decode(s)?;
		if bytes.len() != 32 {
			return Err(KeyError::InvalidLength(bytes.len()));
		}
		let mut arr = [0u8; 32];
		arr.copy_from_slice(&bytes);
		Ok(Self { bytes: arr })
	}

	pub fn from_hex(s: &str) -> Result<Self> {
		let bytes = hex::decode(s)?;
		if bytes.len() != 32 {
			return Err(KeyError::InvalidLength(bytes.len()));
		}
		let mut arr = [0u8; 32];
		arr.copy_from_slice(&bytes);
		Ok(Self { bytes: arr })
	}

	pub fn to_base64(&self) -> Secret<String> {
		Secret::new(STANDARD_NO_PAD.encode(self.bytes))
	}

	pub fn to_hex(&self) -> Secret<String> {
		Secret::new(hex::encode(self.bytes))
	}

	pub fn public_key(&self) -> WgPublicKey {
		let secret = StaticSecret::from(self.bytes);
		let public = PublicKey::from(&secret);
		WgPublicKey {
			bytes: *public.as_bytes(),
		}
	}

	pub fn expose_bytes(&self) -> &[u8; 32] {
		&self.bytes
	}
}

impl fmt::Debug for WgPrivateKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("WgPrivateKey")
			.field("bytes", &"[REDACTED]")
			.finish()
	}
}

impl fmt::Display for WgPrivateKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("[REDACTED]")
	}
}

impl Serialize for WgPrivateKey {
	fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str("[REDACTED]")
	}
}

impl<'de> Deserialize<'de> for WgPrivateKey {
	fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		Self::from_base64(&s).map_err(serde::de::Error::custom)
	}
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WgPublicKey {
	bytes: [u8; 32],
}

impl WgPublicKey {
	pub fn from_bytes(bytes: [u8; 32]) -> Self {
		Self { bytes }
	}

	pub fn from_base64(s: &str) -> Result<Self> {
		let bytes = STANDARD_NO_PAD.decode(s)?;
		if bytes.len() != 32 {
			return Err(KeyError::InvalidLength(bytes.len()));
		}
		let mut arr = [0u8; 32];
		arr.copy_from_slice(&bytes);
		Ok(Self { bytes: arr })
	}

	pub fn from_hex(s: &str) -> Result<Self> {
		let bytes = hex::decode(s)?;
		if bytes.len() != 32 {
			return Err(KeyError::InvalidLength(bytes.len()));
		}
		let mut arr = [0u8; 32];
		arr.copy_from_slice(&bytes);
		Ok(Self { bytes: arr })
	}

	pub fn to_base64(&self) -> String {
		STANDARD_NO_PAD.encode(self.bytes)
	}

	pub fn to_hex(&self) -> String {
		hex::encode(self.bytes)
	}

	pub fn as_bytes(&self) -> &[u8; 32] {
		&self.bytes
	}
}

impl fmt::Debug for WgPublicKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let b64 = self.to_base64();
		let prefix = if b64.len() >= 8 { &b64[..8] } else { &b64 };
		f.debug_struct("WgPublicKey")
			.field("prefix", &format!("{}...", prefix))
			.finish()
	}
}

impl fmt::Display for WgPublicKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.to_base64())
	}
}

impl Serialize for WgPublicKey {
	fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(&self.to_base64())
	}
}

impl<'de> Deserialize<'de> for WgPublicKey {
	fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		Self::from_base64(&s).map_err(serde::de::Error::custom)
	}
}

#[derive(Clone)]
pub struct WgKeyPair {
	private: WgPrivateKey,
	public: WgPublicKey,
}

impl WgKeyPair {
	pub fn generate() -> Self {
		let private = WgPrivateKey::generate();
		let public = private.public_key();
		Self { private, public }
	}

	pub fn from_private_key(private: WgPrivateKey) -> Self {
		let public = private.public_key();
		Self { private, public }
	}

	pub fn from_base64(private_key_base64: &str) -> Result<Self> {
		let private = WgPrivateKey::from_base64(private_key_base64)?;
		Ok(Self::from_private_key(private))
	}

	pub fn from_hex(private_key_hex: &str) -> Result<Self> {
		let private = WgPrivateKey::from_hex(private_key_hex)?;
		Ok(Self::from_private_key(private))
	}

	pub fn private_key(&self) -> &WgPrivateKey {
		&self.private
	}

	pub fn public_key(&self) -> &WgPublicKey {
		&self.public
	}
}

impl fmt::Debug for WgKeyPair {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("WgKeyPair")
			.field("private", &self.private)
			.field("public", &self.public)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn generate_keypair() {
		let keypair = WgKeyPair::generate();
		let public = keypair.public_key();
		assert_eq!(public.as_bytes().len(), 32);
	}

	#[test]
	fn base64_roundtrip() {
		let keypair = WgKeyPair::generate();
		let private_b64 = keypair.private_key().to_base64();
		let restored = WgKeyPair::from_base64(private_b64.expose()).unwrap();
		assert_eq!(keypair.public_key(), restored.public_key());
	}

	#[test]
	fn hex_roundtrip() {
		let keypair = WgKeyPair::generate();
		let private_hex = keypair.private_key().to_hex();
		let restored = WgKeyPair::from_hex(private_hex.expose()).unwrap();
		assert_eq!(keypair.public_key(), restored.public_key());
	}

	#[test]
	fn private_key_debug_is_redacted() {
		let private = WgPrivateKey::generate();
		let debug = format!("{:?}", private);
		assert!(debug.contains("[REDACTED]"));
		assert!(!debug.contains(&private.to_base64().expose().clone()));
	}

	#[test]
	fn private_key_display_is_redacted() {
		let private = WgPrivateKey::generate();
		let display = format!("{}", private);
		assert_eq!(display, "[REDACTED]");
	}

	#[test]
	fn private_key_serialize_is_redacted() {
		let private = WgPrivateKey::generate();
		let json = serde_json::to_string(&private).unwrap();
		assert!(json.contains("[REDACTED]"));
	}

	#[test]
	fn public_key_debug_shows_prefix() {
		let keypair = WgKeyPair::generate();
		let debug = format!("{:?}", keypair.public_key());
		assert!(debug.contains("..."));
	}

	#[test]
	fn public_key_display_shows_full_base64() {
		let keypair = WgKeyPair::generate();
		let display = format!("{}", keypair.public_key());
		assert_eq!(display, keypair.public_key().to_base64());
	}

	#[test]
	fn public_key_serialize_deserialize() {
		let keypair = WgKeyPair::generate();
		let json = serde_json::to_string(keypair.public_key()).unwrap();
		let restored: WgPublicKey = serde_json::from_str(&json).unwrap();
		assert_eq!(keypair.public_key(), &restored);
	}

	proptest! {
		#[test]
		fn private_key_debug_never_leaks(seed in prop::array::uniform32(any::<u8>())) {
			let private = WgPrivateKey::from_bytes(seed);
			let debug = format!("{:?}", private);
			let b64 = STANDARD_NO_PAD.encode(seed);
			let hex_str = hex::encode(seed);

			prop_assert!(!debug.contains(&b64));
			prop_assert!(!debug.contains(&hex_str));
			prop_assert!(debug.contains("[REDACTED]"));
		}

		#[test]
		fn private_key_display_never_leaks(seed in prop::array::uniform32(any::<u8>())) {
			let private = WgPrivateKey::from_bytes(seed);
			let display = format!("{}", private);
			let b64 = STANDARD_NO_PAD.encode(seed);
			let hex_str = hex::encode(seed);

			prop_assert!(!display.contains(&b64));
			prop_assert!(!display.contains(&hex_str));
			prop_assert_eq!(display, "[REDACTED]");
		}

		#[test]
		fn private_key_serialize_never_leaks(seed in prop::array::uniform32(any::<u8>())) {
			let private = WgPrivateKey::from_bytes(seed);
			let json = serde_json::to_string(&private).unwrap();
			let b64 = STANDARD_NO_PAD.encode(seed);
			let hex_str = hex::encode(seed);

			prop_assert!(!json.contains(&b64));
			prop_assert!(!json.contains(&hex_str));
			prop_assert!(json.contains("[REDACTED]"));
		}

		#[test]
		fn keypair_roundtrip_via_base64(seed in prop::array::uniform32(any::<u8>())) {
			let private = WgPrivateKey::from_bytes(seed);
			let keypair = WgKeyPair::from_private_key(private);
			let b64 = keypair.private_key().to_base64();
			let restored = WgKeyPair::from_base64(b64.expose()).unwrap();
			prop_assert_eq!(keypair.public_key(), restored.public_key());
		}

		#[test]
		fn keypair_roundtrip_via_hex(seed in prop::array::uniform32(any::<u8>())) {
			let private = WgPrivateKey::from_bytes(seed);
			let keypair = WgKeyPair::from_private_key(private);
			let hex_str = keypair.private_key().to_hex();
			let restored = WgKeyPair::from_hex(hex_str.expose()).unwrap();
			prop_assert_eq!(keypair.public_key(), restored.public_key());
		}
	}
}
