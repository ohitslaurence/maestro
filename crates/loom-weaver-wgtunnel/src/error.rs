// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
	#[error("engine error: {0}")]
	Engine(#[from] loom_wgtunnel_engine::EngineError),

	#[error("registration error: {0}")]
	Registration(#[from] RegistrationError),

	#[error("peer stream error: {0}")]
	PeerStream(#[from] PeerError),

	#[error("configuration error: {0}")]
	Config(#[from] ConfigError),

	#[error("shutdown requested")]
	Shutdown,
}

#[derive(Debug, Error)]
pub enum RegistrationError {
	#[error("HTTP error: {0}")]
	Http(#[from] reqwest::Error),

	#[error("no SVID available")]
	NoSvid,

	#[error("SVID error: {0}")]
	Svid(String),

	#[error("URL parse error: {0}")]
	Url(#[from] url::ParseError),

	#[error("IP parse error: {0}")]
	IpParse(#[from] std::net::AddrParseError),

	#[error("secrets client error: {0}")]
	SecretsClient(#[from] loom_weaver_secrets::SecretsClientError),
}

#[derive(Debug, Error)]
pub enum PeerError {
	#[error("connection error: {0}")]
	Connection(String),

	#[error("stream ended unexpectedly")]
	StreamEnded,

	#[error("parse error: {0}")]
	Parse(#[from] serde_json::Error),

	#[error("HTTP error: {0}")]
	Http(#[from] reqwest::Error),
}

#[derive(Debug, Error)]
pub enum ConfigError {
	#[error("missing environment variable: {0}")]
	MissingEnv(String),

	#[error("parse error: {0}")]
	Parse(String),
}

pub type Result<T> = std::result::Result<T, DaemonError>;
