// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
	#[error("HTTP request failed: {0}")]
	Http(#[from] reqwest::Error),

	#[error("API error: {status} - {message}")]
	Api { status: u16, message: String },

	#[error("key file error: {0}")]
	KeyFile(#[from] loom_wgtunnel_common::keys_file::KeyFileError),

	#[error("engine error: {0}")]
	Engine(#[from] loom_wgtunnel_engine::EngineError),

	#[error("tunnel not running")]
	TunnelNotRunning,

	#[error("tunnel already running")]
	TunnelAlreadyRunning,

	#[error("device not registered")]
	DeviceNotRegistered,

	#[error("weaver not found: {0}")]
	WeaverNotFound(String),

	#[error("session not found: {0}")]
	SessionNotFound(String),

	#[error("SSH failed with exit code: {0}")]
	SshFailed(i32),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("JSON error: {0}")]
	Json(#[from] serde_json::Error),

	#[error("URL parse error: {0}")]
	UrlParse(#[from] url::ParseError),

	#[error("{0}")]
	Other(String),
}

pub type Result<T> = std::result::Result<T, CliError>;
