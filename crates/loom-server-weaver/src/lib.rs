// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Weaver provisioning business logic for Loom.
//!
//! This crate provides the core business logic for creating, managing, and
//! monitoring ephemeral weaver execution environments using Kubernetes Pods.
//!
//! # Architecture
//!
//! The provisioner layer sits between the HTTP API (loom-server) and the
//! Kubernetes client (loom-k8s), implementing:
//!
//! - Weaver lifecycle management
//! - TTL-based automatic cleanup
//! - Webhook notifications
//! - Resource limit enforcement

pub mod cleanup;
pub mod config;
pub mod error;
pub mod provisioner;
pub mod types;
pub mod webhook;

pub use cleanup::start_cleanup_task;
pub use config::{WeaverConfig, WebhookConfig, WebhookEvent};
pub use error::ProvisionerError;
pub use loom_server_k8s::{AttachedProcess, LogStream};
pub use provisioner::Provisioner;
pub use types::{
	CleanupResult, CreateWeaverRequest, LogStreamOptions, ResourceSpec, Weaver, WeaverId,
	WeaverStatus,
};
pub use webhook::{WebhookDispatcher, WebhookPayload};
