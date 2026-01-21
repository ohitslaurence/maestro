// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use thiserror::Error;

pub type AuditResult<T> = Result<T, AuditError>;

#[derive(Error, Debug)]
pub enum AuditError {
	#[error("event queue is at capacity")]
	QueueFull,

	#[error("sink '{sink}' error: {source}")]
	SinkError {
		sink: String,
		#[source]
		source: AuditSinkError,
	},

	#[error("enrichment error: {0}")]
	EnrichmentError(String),

	#[error("configuration error: {0}")]
	ConfigError(String),

	#[error("service is shutting down")]
	Shutdown,
}

#[derive(Error, Debug)]
pub enum AuditSinkError {
	#[error("transient error: {0}")]
	Transient(String),

	#[error("permanent error: {0}")]
	Permanent(String),
}
