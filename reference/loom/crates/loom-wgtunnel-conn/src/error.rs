// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConnError {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),

	#[error("DERP error: {0}")]
	Derp(#[from] loom_wgtunnel_derp::DerpError),

	#[error("STUN error: {0}")]
	Stun(#[from] crate::stun::StunError),

	#[error("no path to peer {0}")]
	NoPeerPath(String),

	#[error("DERP region {0} not found in map")]
	UnknownDerpRegion(u16),

	#[error("socket not bound")]
	NotBound,
}

pub type Result<T> = std::result::Result<T, ConnError>;
