// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Envelope encryption for secret values.
//!
//! Uses AES-256-GCM for both KEK (master key) and DEK (per-secret key) encryption.

use aes_gcm::{
	aead::{Aead, KeyInit, OsRng},
	Aes256Gcm, Key, Nonce,
};
use rand::RngCore;
use zeroize::{Zeroize, Zeroizing};

use crate::error::{SecretsError, SecretsResult};

/// Size of encryption keys in bytes (256 bits for AES-256).
pub const KEY_SIZE: usize = 32;

/// Size of AES-GCM nonce in bytes.
pub const NONCE_SIZE: usize = 12;

/// Encrypted data with nonce.
#[derive(Debug, Clone)]
pub struct EncryptedData {
	pub ciphertext: Vec<u8>,
	pub nonce: [u8; NONCE_SIZE],
}

/// Generate a random encryption key.
pub fn generate_key() -> Zeroizing<[u8; KEY_SIZE]> {
	let mut key = Zeroizing::new([0u8; KEY_SIZE]);
	OsRng.fill_bytes(key.as_mut());
	key
}

/// Generate a new Data Encryption Key (DEK).
///
/// Alias for `generate_key()` with semantic naming for envelope encryption.
pub fn generate_dek() -> Zeroizing<[u8; KEY_SIZE]> {
	generate_key()
}

/// Generate a random nonce.
///
/// Uses 96-bit random nonces from OsRng. For expected volumes of DEK/secret
/// encryptions this is cryptographically safe, but the same (key, nonce) pair
/// must never be reused. AES-GCM has a 2^-32 collision probability after
/// approximately 2^32 encryptions with the same key - well beyond expected
/// usage patterns. If encryption volumes grow very large under a single key,
/// consider a counter-based nonce scheme.
pub fn generate_nonce() -> [u8; NONCE_SIZE] {
	let mut nonce = [0u8; NONCE_SIZE];
	OsRng.fill_bytes(&mut nonce);
	nonce
}

/// Encrypt a DEK with the KEK.
pub fn encrypt_dek(kek: &[u8; KEY_SIZE], dek: &[u8; KEY_SIZE]) -> SecretsResult<EncryptedData> {
	let key = Key::<Aes256Gcm>::from_slice(kek);
	let cipher = Aes256Gcm::new(key);

	let nonce_bytes = generate_nonce();
	let nonce = Nonce::from_slice(&nonce_bytes);

	let ciphertext = cipher
		.encrypt(nonce, dek.as_slice())
		.map_err(|e| SecretsError::Encryption(format!("DEK encryption failed: {e}")))?;

	Ok(EncryptedData {
		ciphertext,
		nonce: nonce_bytes,
	})
}

/// Decrypt a DEK with the KEK.
pub fn decrypt_dek(
	kek: &[u8; KEY_SIZE],
	encrypted: &EncryptedData,
) -> SecretsResult<Zeroizing<[u8; KEY_SIZE]>> {
	let key = Key::<Aes256Gcm>::from_slice(kek);
	let cipher = Aes256Gcm::new(key);
	let nonce = Nonce::from_slice(&encrypted.nonce);

	let mut plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(
		cipher
			.decrypt(nonce, encrypted.ciphertext.as_slice())
			.map_err(|e| SecretsError::Decryption(format!("DEK decryption failed: {e}")))?,
	);

	if plaintext.len() != KEY_SIZE {
		return Err(SecretsError::InvalidKeySize {
			expected: KEY_SIZE,
			actual: plaintext.len(),
		});
	}

	let mut dek = Zeroizing::new([0u8; KEY_SIZE]);
	dek.copy_from_slice(&plaintext);
	plaintext.zeroize();
	Ok(dek)
}

/// Encrypt a secret value with a DEK.
pub fn encrypt_secret_value(
	dek: &[u8; KEY_SIZE],
	plaintext: &[u8],
) -> SecretsResult<EncryptedData> {
	let key = Key::<Aes256Gcm>::from_slice(dek);
	let cipher = Aes256Gcm::new(key);

	let nonce_bytes = generate_nonce();
	let nonce = Nonce::from_slice(&nonce_bytes);

	let ciphertext = cipher
		.encrypt(nonce, plaintext)
		.map_err(|e| SecretsError::Encryption(format!("secret encryption failed: {e}")))?;

	Ok(EncryptedData {
		ciphertext,
		nonce: nonce_bytes,
	})
}

/// Decrypt a secret value with a DEK.
pub fn decrypt_secret_value(
	dek: &[u8; KEY_SIZE],
	encrypted: &EncryptedData,
) -> SecretsResult<Zeroizing<Vec<u8>>> {
	let key = Key::<Aes256Gcm>::from_slice(dek);
	let cipher = Aes256Gcm::new(key);
	let nonce = Nonce::from_slice(&encrypted.nonce);

	let plaintext = cipher
		.decrypt(nonce, encrypted.ciphertext.as_slice())
		.map_err(|e| SecretsError::Decryption(format!("secret decryption failed: {e}")))?;

	Ok(Zeroizing::new(plaintext))
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn key_generation_produces_unique_keys() {
		let key1 = generate_key();
		let key2 = generate_key();
		assert_ne!(key1.as_slice(), key2.as_slice());
	}

	#[test]
	fn generate_dek_produces_valid_key() {
		let dek = generate_dek();
		assert_eq!(dek.len(), KEY_SIZE);
	}

	#[test]
	fn dek_encryption_roundtrip() {
		let kek = generate_key();
		let dek = generate_key();

		let encrypted = encrypt_dek(&kek, &dek).unwrap();
		let decrypted = decrypt_dek(&kek, &encrypted).unwrap();

		assert_eq!(dek.as_slice(), decrypted.as_slice());
	}

	#[test]
	fn secret_encryption_roundtrip() {
		let dek = generate_key();
		let plaintext = b"super secret value";

		let encrypted = encrypt_secret_value(&dek, plaintext).unwrap();
		let decrypted = decrypt_secret_value(&dek, &encrypted).unwrap();

		assert_eq!(plaintext.as_slice(), decrypted.as_slice());
	}

	#[test]
	fn wrong_key_fails_decryption() {
		let kek1 = generate_key();
		let kek2 = generate_key();
		let dek = generate_key();

		let encrypted = encrypt_dek(&kek1, &dek).unwrap();
		let result = decrypt_dek(&kek2, &encrypted);

		assert!(result.is_err());
	}

	#[test]
	fn tampered_ciphertext_fails() {
		let dek = generate_key();
		let plaintext = b"secret";

		let mut encrypted = encrypt_secret_value(&dek, plaintext).unwrap();
		if !encrypted.ciphertext.is_empty() {
			encrypted.ciphertext[0] ^= 0xFF;
		}

		let result = decrypt_secret_value(&dek, &encrypted);
		assert!(result.is_err());
	}

	proptest! {
		#[test]
		fn prop_dek_encryption_roundtrip(dek_bytes in proptest::collection::vec(any::<u8>(), KEY_SIZE)) {
			let kek = generate_key();
			let dek: [u8; KEY_SIZE] = dek_bytes.try_into().unwrap();

			let encrypted = encrypt_dek(&kek, &dek).unwrap();
			let decrypted = decrypt_dek(&kek, &encrypted).unwrap();

			prop_assert_eq!(dek.as_slice(), decrypted.as_slice());
		}

		#[test]
		fn prop_secret_encryption_roundtrip(plaintext in proptest::collection::vec(any::<u8>(), 0..10000)) {
			let dek = generate_key();

			let encrypted = encrypt_secret_value(&dek, &plaintext).unwrap();
			let decrypted = decrypt_secret_value(&dek, &encrypted).unwrap();

			prop_assert_eq!(plaintext, decrypted.as_slice());
		}

		#[test]
		fn prop_encrypted_data_has_correct_nonce_size(plaintext in proptest::collection::vec(any::<u8>(), 0..1000)) {
			let dek = generate_key();

			let encrypted = encrypt_secret_value(&dek, &plaintext).unwrap();

			prop_assert_eq!(encrypted.nonce.len(), NONCE_SIZE);
		}

		#[test]
		fn prop_different_encryptions_produce_different_ciphertexts(plaintext in proptest::collection::vec(any::<u8>(), 1..1000)) {
			let dek = generate_key();

			let encrypted1 = encrypt_secret_value(&dek, &plaintext).unwrap();
			let encrypted2 = encrypt_secret_value(&dek, &plaintext).unwrap();

			prop_assert_ne!(encrypted1.nonce, encrypted2.nonce);
			prop_assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
		}

		#[test]
		fn prop_tampered_ciphertext_fails_decryption(
			plaintext in proptest::collection::vec(any::<u8>(), 1..1000),
			tamper_idx in 0usize..1000usize,
		) {
			let dek = generate_key();

			let mut encrypted = encrypt_secret_value(&dek, &plaintext).unwrap();
			let idx = tamper_idx % encrypted.ciphertext.len();
			encrypted.ciphertext[idx] ^= 0xFF;

			let result = decrypt_secret_value(&dek, &encrypted);
			prop_assert!(result.is_err());
		}

		#[test]
		fn prop_wrong_key_fails_dek_decryption(
			dek_bytes in proptest::collection::vec(any::<u8>(), KEY_SIZE)
		) {
			let kek1 = generate_key();
			let kek2 = generate_key();
			let dek: [u8; KEY_SIZE] = dek_bytes.try_into().unwrap();

			let encrypted = encrypt_dek(&kek1, &dek).unwrap();
			let result = decrypt_dek(&kek2, &encrypted);

			prop_assert!(result.is_err());
		}
	}
}
