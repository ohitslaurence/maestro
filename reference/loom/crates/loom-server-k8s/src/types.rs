// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::pin::Pin;

use bytes::Bytes;
use futures::Stream;
use tokio::io::{AsyncRead, AsyncWrite};

pub use k8s_openapi::api::core::v1::{
	Container, ContainerPort, EmptyDirVolumeSource, EnvVar, EnvVarSource, HostPathVolumeSource,
	LocalObjectReference, Namespace, ObjectFieldSelector, Pod, PodSpec, PodStatus,
	ResourceRequirements, SecurityContext, Volume, VolumeMount,
};

/// Options for log streaming.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
	pub tail: u32,
	pub timestamps: bool,
}

/// A pinned stream of log lines from a container.
pub type LogStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

/// Bidirectional stream for container I/O via exec/attach.
pub struct AttachedProcess {
	pub stdin: Pin<Box<dyn AsyncWrite + Send>>,
	pub stdout: Pin<Box<dyn AsyncRead + Send>>,
}
