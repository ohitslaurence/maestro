// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),

	#[error("connection error: {0}")]
	Conn(#[from] loom_wgtunnel_conn::ConnError),

	#[error("WireGuard error: {0}")]
	WireGuard(String),

	#[error("peer not found: {0}")]
	PeerNotFound(String),

	#[error("already running")]
	AlreadyRunning,

	#[error("not running")]
	NotRunning,

	#[error("virtual device error: {0}")]
	Device(String),

	#[error("TCP connection failed: {0}")]
	TcpConnect(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;
