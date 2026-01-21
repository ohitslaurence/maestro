// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

pub const MAX_FRAME_SIZE: usize = 1 << 20; // 1 MiB

#[derive(Debug, Error)]
pub enum DerpError {
	#[error("connection failed: {0}")]
	Connection(#[source] std::io::Error),

	#[error("TLS error: {0}")]
	Tls(String),

	#[error("invalid frame type: {0}")]
	InvalidFrameType(u8),

	#[error("frame too large: {0} bytes (max {1})")]
	FrameTooLarge(usize, usize),

	#[error("payload too short for frame type {0}")]
	PayloadTooShort(u8),

	#[error("unexpected frame type: expected {expected}, got {actual}")]
	UnexpectedFrameType { expected: &'static str, actual: u8 },

	#[error("handshake failed: {0}")]
	Handshake(String),

	#[error("JSON decode error: {0}")]
	Json(#[from] serde_json::Error),

	#[error("connection closed")]
	ConnectionClosed,

	#[error("HTTP error: {0}")]
	Http(#[from] reqwest::Error),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DerpError>;
