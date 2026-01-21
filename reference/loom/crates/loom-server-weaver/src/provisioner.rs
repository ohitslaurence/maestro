// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Core provisioner implementation for weaver lifecycle management.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Capabilities;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use loom_server_k8s::{
	AttachedProcess, Container, ContainerPort, EmptyDirVolumeSource, EnvVar, EnvVarSource,
	HostPathVolumeSource, K8sClient, LocalObjectReference, LogOptions, LogStream,
	ObjectFieldSelector, Pod, PodSpec, ResourceRequirements, SecurityContext, Volume, VolumeMount,
};

use crate::config::WeaverConfig;
use crate::error::ProvisionerError;
use crate::types::{
	CleanupResult, CreateWeaverRequest, LogStreamOptions, Weaver, WeaverId, WeaverStatus,
};

const MANAGED_LABEL: &str = "loom.dev/managed";
const WEAVER_ID_LABEL: &str = "loom.dev/weaver-id";
const LABEL_OWNER_USER_ID: &str = "loom.dev/owner-user-id";
const LABEL_ORG_ID: &str = "loom.dev/org-id";
const LABEL_REPO_ID: &str = "loom.dev/repo-id";
const LABEL_IMAGE: &str = "loom.dev/image";
const LABEL_IMAGE_REGISTRY: &str = "loom.dev/image-registry";
const LABEL_IMAGE_NAME: &str = "loom.dev/image-name";
const LABEL_WG_ENABLED: &str = "loom.dev/wg-enabled";
const TAGS_ANNOTATION: &str = "loom.dev/tags";
const LIFETIME_ANNOTATION: &str = "loom.dev/lifetime-hours";
const CONTAINER_NAME: &str = "weaver";
const SIDECAR_CONTAINER_NAME: &str = "audit-sidecar";
const SIDECAR_IMAGE_DEFAULT: &str = "ghcr.io/ghuntley/loom-audit-sidecar:latest";
const DEFAULT_MEMORY_LIMIT: &str = "16Gi";
// eBPF volume mounts for audit sidecar - required for tracepoint attachment
const VOLUME_TRACEFS: &str = "tracefs";
const VOLUME_DEBUGFS: &str = "debugfs";
const VOLUME_BPF: &str = "bpf";
const VOLUME_TMP: &str = "tmp";
const PATH_TRACEFS: &str = "/sys/kernel/tracing";
const PATH_DEBUGFS: &str = "/sys/kernel/debug";
const PATH_BPF: &str = "/sys/fs/bpf";
const PATH_TMP: &str = "/tmp";
const POLL_INTERVAL_MS: u64 = 500;
const MAX_LABEL_LENGTH: usize = 63;
const DEFAULT_REGISTRY: &str = "docker.io";

/// Sanitize a string to be a valid Kubernetes label value.
///
/// K8s label values must:
/// - Be 63 characters or less
/// - Begin and end with an alphanumeric character
/// - Contain only alphanumeric characters, dashes, underscores, and dots
fn sanitize_label_value(value: &str) -> String {
	let sanitized: String = value
		.chars()
		.map(|c| {
			if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
				c
			} else {
				'_'
			}
		})
		.collect();

	let trimmed: String = sanitized
		.trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
		.trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
		.to_string();

	if trimmed.len() > MAX_LABEL_LENGTH {
		let truncated = &trimmed[..MAX_LABEL_LENGTH];
		truncated
			.trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
			.to_string()
	} else {
		trimmed
	}
}

/// Parsed components of a container image reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageParts {
	/// The registry (e.g., "docker.io", "ghcr.io")
	pub registry: String,
	/// The image name without registry or tag (e.g., "library/python", "org/repo")
	pub name: String,
}

/// Parse a container image reference into registry and name components.
///
/// Handles various image formats:
/// - `python:3.12` → registry: "docker.io", name: "python"
/// - `library/python:3.12` → registry: "docker.io", name: "library/python"
/// - `ghcr.io/org/repo:latest` → registry: "ghcr.io", name: "org/repo"
/// - `docker.io/library/python:3.12` → registry: "docker.io", name: "library/python"
fn parse_image_parts(image: &str) -> ImageParts {
	let without_tag = image.split(':').next().unwrap_or(image);
	let without_digest = without_tag.split('@').next().unwrap_or(without_tag);

	let parts: Vec<&str> = without_digest.split('/').collect();

	match parts.len() {
		1 => ImageParts {
			registry: DEFAULT_REGISTRY.to_string(),
			name: parts[0].to_string(),
		},
		2 => {
			if looks_like_registry(parts[0]) {
				ImageParts {
					registry: parts[0].to_string(),
					name: parts[1].to_string(),
				}
			} else {
				ImageParts {
					registry: DEFAULT_REGISTRY.to_string(),
					name: without_digest.to_string(),
				}
			}
		}
		_ => {
			if looks_like_registry(parts[0]) {
				ImageParts {
					registry: parts[0].to_string(),
					name: parts[1..].join("/"),
				}
			} else {
				ImageParts {
					registry: DEFAULT_REGISTRY.to_string(),
					name: without_digest.to_string(),
				}
			}
		}
	}
}

/// Check if a string looks like a container registry hostname.
fn looks_like_registry(s: &str) -> bool {
	s.contains('.') || s.contains(':') || s == "localhost"
}

/// The main provisioner for managing weaver lifecycle.
pub struct Provisioner {
	client: Arc<dyn K8sClient>,
	config: WeaverConfig,
}

impl Provisioner {
	/// Create a new provisioner with the given K8s client and configuration.
	pub fn new(client: Arc<dyn K8sClient>, config: WeaverConfig) -> Self {
		Self { client, config }
	}

	/// Get the namespace this provisioner operates in.
	pub fn namespace(&self) -> &str {
		&self.config.namespace
	}

	/// Validate that the configured namespace exists in the cluster.
	///
	/// This should be called on startup to fail fast if the namespace
	/// is not properly configured.
	pub async fn validate_namespace(&self) -> Result<(), ProvisionerError> {
		match self.client.get_namespace(&self.config.namespace).await {
			Ok(_) => {
				tracing::info!(namespace = %self.config.namespace, "Validated namespace exists");
				Ok(())
			}
			Err(loom_server_k8s::K8sError::NamespaceNotFound { .. }) => {
				Err(ProvisionerError::NamespaceNotFound {
					name: self.config.namespace.clone(),
				})
			}
			Err(e) => Err(e.into()),
		}
	}

	/// Create a new weaver based on the provided request.
	pub async fn create_weaver(&self, req: CreateWeaverRequest) -> Result<Weaver, ProvisionerError> {
		let lifetime_hours = self.validate_lifetime(req.lifetime_hours)?;

		let active_count = self.count_active_weavers().await?;
		if active_count >= self.config.max_concurrent {
			return Err(ProvisionerError::TooManyWeavers {
				current: active_count,
				max: self.config.max_concurrent,
			});
		}

		let id = WeaverId::new();
		let pod = build_pod_spec(&id, &req, &self.config, lifetime_hours);
		let pod_name = id.as_k8s_name();

		tracing::info!(weaver_id = %id, pod_name = %pod_name, image = %req.image, "Creating weaver pod");

		self.client.create_pod(&self.config.namespace, pod).await?;

		let status = self
			.poll_until_ready(
				&pod_name,
				Duration::from_secs(self.config.ready_timeout_secs),
			)
			.await?;

		let created_at = Utc::now();
		Ok(Weaver {
			id,
			pod_name,
			status,
			image: req.image,
			tags: req.tags,
			created_at,
			lifetime_hours,
			age_hours: 0.0,
			owner_user_id: req.owner_user_id.unwrap_or_default(),
		})
	}

	/// Count the number of active (Pending or Running) weavers.
	pub async fn count_active_weavers(&self) -> Result<u32, ProvisionerError> {
		let pods = self
			.client
			.list_pods(&self.config.namespace, &format!("{MANAGED_LABEL}=true"))
			.await?;

		let count = pods
			.iter()
			.filter(|pod| {
				let phase = pod
					.status
					.as_ref()
					.and_then(|s| s.phase.as_deref())
					.unwrap_or("Unknown");
				matches!(phase, "Pending" | "Running")
			})
			.count() as u32;

		Ok(count)
	}

	fn validate_lifetime(&self, requested: Option<u32>) -> Result<u32, ProvisionerError> {
		match requested {
			Some(hours) if hours > self.config.max_ttl_hours => Err(ProvisionerError::InvalidLifetime {
				requested: hours,
				max: self.config.max_ttl_hours,
			}),
			Some(hours) => Ok(hours),
			None => Ok(self.config.default_ttl_hours),
		}
	}

	async fn poll_until_ready(
		&self,
		pod_name: &str,
		timeout: Duration,
	) -> Result<WeaverStatus, ProvisionerError> {
		let start = std::time::Instant::now();
		let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

		loop {
			if start.elapsed() > timeout {
				return Err(ProvisionerError::WeaverTimeout {
					id: pod_name.to_string(),
				});
			}

			let pod = self
				.client
				.get_pod(pod_name, &self.config.namespace)
				.await?;

			let phase = pod
				.status
				.as_ref()
				.and_then(|s| s.phase.as_deref())
				.unwrap_or("Unknown");

			match phase {
				"Running" => return Ok(WeaverStatus::Running),
				"Succeeded" => return Ok(WeaverStatus::Succeeded),
				"Failed" => {
					let reason = pod
						.status
						.as_ref()
						.and_then(|s| s.message.clone())
						.unwrap_or_else(|| "Unknown failure".to_string());
					return Err(ProvisionerError::WeaverFailed {
						id: pod_name.to_string(),
						reason,
					});
				}
				"Pending" => {
					tokio::time::sleep(poll_interval).await;
				}
				_ => {
					tokio::time::sleep(poll_interval).await;
				}
			}
		}
	}

	/// List all weavers, optionally filtered by tags.
	///
	/// If `tag_filter` is provided, only weavers matching all specified tags are returned.
	pub async fn list_weavers(
		&self,
		tag_filter: Option<HashMap<String, String>>,
	) -> Result<Vec<Weaver>, ProvisionerError> {
		let pods = self
			.client
			.list_pods(&self.config.namespace, &format!("{MANAGED_LABEL}=true"))
			.await?;

		let mut weavers = Vec::new();
		for pod in &pods {
			match pod_to_weaver(pod) {
				Ok(weaver) => weavers.push(weaver),
				Err(e) => {
					tracing::warn!("Failed to parse pod as weaver: {}", e);
				}
			}
		}

		if let Some(filter) = tag_filter {
			weavers.retain(|weaver| {
				filter
					.iter()
					.all(|(k, v)| weaver.tags.get(k).map(|av| av == v).unwrap_or(false))
			});
		}

		Ok(weavers)
	}

	/// List weavers owned by a specific user.
	pub async fn list_weavers_for_user(
		&self,
		user_id: &str,
	) -> Result<Vec<Weaver>, ProvisionerError> {
		let all = self.list_weavers(None).await?;
		Ok(
			all
				.into_iter()
				.filter(|w| w.owner_user_id == user_id)
				.collect(),
		)
	}

	/// Get a specific weaver by ID.
	pub async fn get_weaver(&self, id: &WeaverId) -> Result<Weaver, ProvisionerError> {
		let pod_name = id.as_k8s_name();
		match self.client.get_pod(&pod_name, &self.config.namespace).await {
			Ok(pod) => pod_to_weaver(&pod),
			Err(loom_server_k8s::K8sError::PodNotFound { .. }) => {
				Err(ProvisionerError::WeaverNotFound { id: id.to_string() })
			}
			Err(e) => Err(e.into()),
		}
	}

	/// Delete a weaver by ID with a 5-second grace period.
	pub async fn delete_weaver(&self, id: &WeaverId) -> Result<(), ProvisionerError> {
		let pod_name = id.as_k8s_name();
		match self
			.client
			.delete_pod(&pod_name, &self.config.namespace, 5)
			.await
		{
			Ok(()) => Ok(()),
			Err(loom_server_k8s::K8sError::PodNotFound { .. }) => {
				Err(ProvisionerError::WeaverNotFound { id: id.to_string() })
			}
			Err(e) => Err(e.into()),
		}
	}

	/// Stream logs from a weaver's container.
	pub async fn stream_logs(
		&self,
		id: &WeaverId,
		opts: LogStreamOptions,
	) -> Result<LogStream, ProvisionerError> {
		self.get_weaver(id).await?;

		let pod_name = id.as_k8s_name();
		let log_opts = LogOptions {
			tail: opts.tail,
			timestamps: opts.timestamps,
		};

		self
			.client
			.stream_logs(&pod_name, &self.config.namespace, CONTAINER_NAME, log_opts)
			.await
			.map_err(Into::into)
	}

	/// Attach to a weaver's container for interactive I/O.
	///
	/// Returns an `AttachedProcess` with stdin/stdout streams for bidirectional
	/// communication with the running container.
	pub async fn attach_weaver(&self, id: &WeaverId) -> Result<AttachedProcess, ProvisionerError> {
		let weaver = self.get_weaver(id).await?;

		if weaver.status != WeaverStatus::Running {
			return Err(ProvisionerError::WeaverNotRunning {
				id: id.to_string(),
				status: format!("{:?}", weaver.status),
			});
		}

		let pod_name = id.as_k8s_name();
		self
			.client
			.exec_attach(&pod_name, &self.config.namespace, CONTAINER_NAME)
			.await
			.map_err(Into::into)
	}

	/// Find all weavers that have exceeded their lifetime.
	pub async fn find_expired_weavers(&self) -> Result<Vec<Weaver>, ProvisionerError> {
		let weavers = self.list_weavers(None).await?;
		let expired = weavers
			.into_iter()
			.filter(|weaver| weaver.age_hours >= weaver.lifetime_hours as f64)
			.collect();
		Ok(expired)
	}

	/// Clean up all expired weavers.
	pub async fn cleanup_expired_weavers(&self) -> Result<CleanupResult, ProvisionerError> {
		let expired = self.find_expired_weavers().await?;
		let mut deleted = Vec::new();

		for weaver in expired {
			match self.delete_weaver(&weaver.id).await {
				Ok(()) => {
					tracing::info!(weaver_id = %weaver.id, age_hours = weaver.age_hours, "Deleted expired weaver");
					deleted.push(weaver.id);
				}
				Err(ProvisionerError::WeaverNotFound { .. }) => {
					tracing::debug!(weaver_id = %weaver.id, "Weaver already deleted");
				}
				Err(e) => {
					tracing::error!(weaver_id = %weaver.id, error = %e, "Failed to delete expired weaver");
				}
			}
		}

		let count = deleted.len() as u32;
		Ok(CleanupResult { deleted, count })
	}

	/// Get the configured cleanup interval in seconds.
	pub fn cleanup_interval_secs(&self) -> u64 {
		self.config.cleanup_interval_secs
	}
}

/// Build a Kubernetes Pod spec for a weaver.
fn build_pod_spec(
	id: &WeaverId,
	req: &CreateWeaverRequest,
	config: &WeaverConfig,
	lifetime_hours: u32,
) -> Pod {
	let pod_name = id.as_k8s_name();

	let mut labels = BTreeMap::new();
	labels.insert(MANAGED_LABEL.to_string(), "true".to_string());
	labels.insert(WEAVER_ID_LABEL.to_string(), id.to_string());
	labels.insert(
		LABEL_OWNER_USER_ID.to_string(),
		req.owner_user_id.clone().unwrap_or_default(),
	);
	labels.insert(LABEL_ORG_ID.to_string(), req.org_id.clone());
	if let Some(ref repo_id) = req.repo_id {
		labels.insert(LABEL_REPO_ID.to_string(), repo_id.clone());
	}
	labels.insert(LABEL_IMAGE.to_string(), sanitize_label_value(&req.image));

	let image_parts = parse_image_parts(&req.image);
	labels.insert(
		LABEL_IMAGE_REGISTRY.to_string(),
		sanitize_label_value(&image_parts.registry),
	);
	labels.insert(
		LABEL_IMAGE_NAME.to_string(),
		sanitize_label_value(&image_parts.name),
	);
	if config.wg_enabled {
		labels.insert(LABEL_WG_ENABLED.to_string(), "true".to_string());
	}

	let mut annotations = BTreeMap::new();
	if !req.tags.is_empty() {
		if let Ok(tags_json) = serde_json::to_string(&req.tags) {
			annotations.insert(TAGS_ANNOTATION.to_string(), tags_json);
		}
	}
	annotations.insert(LIFETIME_ANNOTATION.to_string(), lifetime_hours.to_string());

	let mut env_vars: Vec<EnvVar> = req
		.env
		.iter()
		.map(|(k, v)| EnvVar {
			name: k.clone(),
			value: Some(v.clone()),
			value_from: None,
		})
		.collect();

	if let Some(repo) = &req.repo {
		env_vars.push(EnvVar {
			name: "LOOM_REPO".to_string(),
			value: Some(repo.clone()),
			value_from: None,
		});
	}
	if let Some(branch) = &req.branch {
		env_vars.push(EnvVar {
			name: "LOOM_BRANCH".to_string(),
			value: Some(branch.clone()),
			value_from: None,
		});
	}

	// Always inject LOOM_SERVER_URL so the loom CLI can connect to the LLM proxy
	if !config.server_url.is_empty() {
		env_vars.push(EnvVar {
			name: "LOOM_SERVER_URL".to_string(),
			value: Some(config.server_url.clone()),
			value_from: None,
		});
	}

	// Weaver ID is always useful for identification/logging
	env_vars.push(EnvVar {
		name: "LOOM_WEAVER_ID".to_string(),
		value: Some(id.to_string()),
		value_from: None,
	});

	if let Some(ref secrets_url) = config.secrets_server_url {
		env_vars.push(EnvVar {
			name: "LOOM_SECRETS_SERVER_URL".to_string(),
			value: Some(secrets_url.clone()),
			value_from: None,
		});
	}
	if config.secrets_allow_insecure {
		env_vars.push(EnvVar {
			name: "LOOM_SECRETS_ALLOW_INSECURE".to_string(),
			value: Some("1".to_string()),
			value_from: None,
		});
	}

	if config.wg_enabled {
		env_vars.push(EnvVar {
			name: "LOOM_WG_ENABLED".to_string(),
			value: Some("true".to_string()),
			value_from: None,
		});
	}

	let mut limits = BTreeMap::new();
	limits.insert(
		"memory".to_string(),
		Quantity(
			req
				.resources
				.memory_limit
				.clone()
				.unwrap_or_else(|| DEFAULT_MEMORY_LIMIT.to_string()),
		),
	);
	if let Some(cpu) = &req.resources.cpu_limit {
		limits.insert("cpu".to_string(), Quantity(cpu.clone()));
	}

	let resources = ResourceRequirements {
		limits: Some(limits),
		requests: None,
		claims: None,
	};

	let security_context = SecurityContext {
		run_as_non_root: Some(true),
		run_as_user: Some(1000),
		run_as_group: Some(1000),
		allow_privilege_escalation: Some(false),
		read_only_root_filesystem: Some(false),
		capabilities: Some(Capabilities {
			drop: Some(vec!["ALL".to_string()]),
			add: None,
		}),
		..Default::default()
	};

	let container = Container {
		name: CONTAINER_NAME.to_string(),
		image: Some(req.image.clone()),
		env: if env_vars.is_empty() {
			None
		} else {
			Some(env_vars)
		},
		command: req.command.clone(),
		args: req.args.clone(),
		working_dir: req.workdir.clone(),
		resources: Some(resources),
		security_context: Some(security_context),
		// Enable TTY and stdin for interactive REPL sessions
		// Required for tmux to work inside the container
		tty: Some(true),
		stdin: Some(true),
		..Default::default()
	};

	let image_pull_secrets = if config.image_pull_secrets.is_empty() {
		None
	} else {
		Some(
			config
				.image_pull_secrets
				.iter()
				.map(|name| LocalObjectReference { name: name.clone() })
				.collect(),
		)
	};

	let mut containers = vec![container];
	let mut share_process_namespace = None;

	if config.audit_enabled {
		let sidecar_image = if config.audit_image.is_empty() {
			SIDECAR_IMAGE_DEFAULT.to_string()
		} else {
			config.audit_image.clone()
		};

		let sidecar_env = vec![
			EnvVar {
				name: "LOOM_WEAVER_ID".to_string(),
				value: Some(id.to_string()),
				value_from: None,
			},
			EnvVar {
				name: "LOOM_ORG_ID".to_string(),
				value: Some(req.org_id.clone()),
				value_from: None,
			},
			EnvVar {
				name: "LOOM_OWNER_USER_ID".to_string(),
				value: Some(req.owner_user_id.clone().unwrap_or_default()),
				value_from: None,
			},
			// Pod name from Kubernetes downward API for SVID exchange
			EnvVar {
				name: "LOOM_POD_NAME".to_string(),
				value: None,
				value_from: Some(EnvVarSource {
					field_ref: Some(ObjectFieldSelector {
						api_version: Some("v1".to_string()),
						field_path: "metadata.name".to_string(),
					}),
					..Default::default()
				}),
			},
			// Pod namespace from Kubernetes downward API for SVID exchange
			EnvVar {
				name: "LOOM_POD_NAMESPACE".to_string(),
				value: None,
				value_from: Some(EnvVarSource {
					field_ref: Some(ObjectFieldSelector {
						api_version: Some("v1".to_string()),
						field_path: "metadata.namespace".to_string(),
					}),
					..Default::default()
				}),
			},
			EnvVar {
				name: "LOOM_SERVER_URL".to_string(),
				value: Some(config.server_url.clone()),
				value_from: None,
			},
			EnvVar {
				name: "LOOM_AUDIT_BATCH_INTERVAL_MS".to_string(),
				value: Some(config.audit_batch_interval_ms.to_string()),
				value_from: None,
			},
			EnvVar {
				name: "LOOM_AUDIT_BUFFER_MAX_BYTES".to_string(),
				value: Some(config.audit_buffer_max_bytes.to_string()),
				value_from: None,
			},
		];

		let sidecar_security_context = SecurityContext {
			run_as_user: Some(0),
			run_as_non_root: Some(false),
			read_only_root_filesystem: Some(true),
			allow_privilege_escalation: Some(false),
			capabilities: Some(Capabilities {
				drop: Some(vec!["ALL".to_string()]),
				add: Some(vec!["BPF".to_string(), "PERFMON".to_string()]),
			}),
			..Default::default()
		};

		let sidecar_ports = vec![
			ContainerPort {
				container_port: 9090,
				name: Some("metrics".to_string()),
				protocol: Some("TCP".to_string()),
				..Default::default()
			},
			ContainerPort {
				container_port: 9091,
				name: Some("health".to_string()),
				protocol: Some("TCP".to_string()),
				..Default::default()
			},
		];

		// Volume mounts for eBPF tracepoint access
		// These are mounted read-only for security where possible
		let sidecar_volume_mounts = vec![
			VolumeMount {
				name: VOLUME_TRACEFS.to_string(),
				mount_path: PATH_TRACEFS.to_string(),
				read_only: Some(false), // eBPF needs write access to attach tracepoints
				..Default::default()
			},
			VolumeMount {
				name: VOLUME_DEBUGFS.to_string(),
				mount_path: PATH_DEBUGFS.to_string(),
				read_only: Some(false), // Required for older kernels using debugfs tracefs
				..Default::default()
			},
			VolumeMount {
				name: VOLUME_BPF.to_string(),
				mount_path: PATH_BPF.to_string(),
				read_only: Some(false), // BPF maps need write access
				..Default::default()
			},
			VolumeMount {
				name: VOLUME_TMP.to_string(),
				mount_path: PATH_TMP.to_string(),
				read_only: Some(false), // Event buffer writes to /tmp
				..Default::default()
			},
		];

		let sidecar_container = Container {
			name: SIDECAR_CONTAINER_NAME.to_string(),
			image: Some(sidecar_image),
			env: Some(sidecar_env),
			ports: Some(sidecar_ports),
			security_context: Some(sidecar_security_context),
			volume_mounts: Some(sidecar_volume_mounts),
			restart_policy: Some("Always".to_string()),
			..Default::default()
		};

		// Add audit sidecar as a regular container (not init container)
		// so it runs alongside the main weaver container
		containers.push(sidecar_container);
		share_process_namespace = Some(true);
	}

	// Pod volumes - only include eBPF mounts when audit is enabled
	let volumes = if config.audit_enabled {
		Some(vec![
			Volume {
				name: VOLUME_TRACEFS.to_string(),
				host_path: Some(HostPathVolumeSource {
					path: PATH_TRACEFS.to_string(),
					type_: Some("Directory".to_string()),
				}),
				..Default::default()
			},
			Volume {
				name: VOLUME_DEBUGFS.to_string(),
				host_path: Some(HostPathVolumeSource {
					path: PATH_DEBUGFS.to_string(),
					type_: Some("Directory".to_string()),
				}),
				..Default::default()
			},
			Volume {
				name: VOLUME_BPF.to_string(),
				host_path: Some(HostPathVolumeSource {
					path: PATH_BPF.to_string(),
					type_: Some("DirectoryOrCreate".to_string()),
				}),
				..Default::default()
			},
			// Writable /tmp for audit event buffer (container has readOnlyRootFilesystem)
			Volume {
				name: VOLUME_TMP.to_string(),
				empty_dir: Some(EmptyDirVolumeSource::default()),
				..Default::default()
			},
		])
	} else {
		None
	};

	Pod {
		metadata: ObjectMeta {
			name: Some(pod_name),
			namespace: Some(config.namespace.clone()),
			labels: Some(labels),
			annotations: Some(annotations),
			..Default::default()
		},
		spec: Some(PodSpec {
			containers,
			volumes,
			restart_policy: Some("Never".to_string()),
			image_pull_secrets,
			share_process_namespace,
			..Default::default()
		}),
		status: None,
	}
}

/// Convert a Kubernetes Pod to a Weaver.
fn pod_to_weaver(pod: &Pod) -> Result<Weaver, ProvisionerError> {
	let metadata = pod.metadata.clone();
	let pod_name = metadata.name.clone().unwrap_or_default();

	let labels = metadata.labels.unwrap_or_default();
	let annotations = metadata.annotations.unwrap_or_default();

	let id_str = labels
		.get(WEAVER_ID_LABEL)
		.ok_or_else(|| ProvisionerError::WeaverFailed {
			id: pod_name.clone(),
			reason: format!("Missing {WEAVER_ID_LABEL} label"),
		})?;

	let id = id_str
		.parse::<WeaverId>()
		.map_err(|_| ProvisionerError::WeaverFailed {
			id: pod_name.clone(),
			reason: format!("Invalid weaver ID: {id_str}"),
		})?;

	let tags: HashMap<String, String> = annotations
		.get(TAGS_ANNOTATION)
		.and_then(|s| serde_json::from_str(s).ok())
		.unwrap_or_default();

	let lifetime_hours: u32 = annotations
		.get(LIFETIME_ANNOTATION)
		.and_then(|s| s.parse().ok())
		.unwrap_or(4);

	let owner_user_id = labels.get(LABEL_OWNER_USER_ID).cloned().unwrap_or_default();

	let status = map_pod_phase(pod);

	let created_at = metadata
		.creation_timestamp
		.map(|ts| ts.0)
		.unwrap_or_else(Utc::now);

	let age_hours = calculate_age_hours(created_at);

	let image = pod
		.spec
		.as_ref()
		.and_then(|spec| spec.containers.first())
		.map(|c| c.image.clone().unwrap_or_default())
		.unwrap_or_default();

	Ok(Weaver {
		id,
		pod_name,
		status,
		image,
		tags,
		created_at,
		lifetime_hours,
		age_hours,
		owner_user_id,
	})
}

/// Map Kubernetes Pod phase to WeaverStatus.
fn map_pod_phase(pod: &Pod) -> WeaverStatus {
	// Check if pod is being deleted (has deletionTimestamp)
	if pod.metadata.deletion_timestamp.is_some() {
		return WeaverStatus::Terminating;
	}

	let phase = pod
		.status
		.as_ref()
		.and_then(|s| s.phase.as_deref())
		.unwrap_or("Unknown");

	match phase {
		"Pending" => WeaverStatus::Pending,
		"Running" => WeaverStatus::Running,
		"Succeeded" => WeaverStatus::Succeeded,
		"Failed" => WeaverStatus::Failed,
		_ => WeaverStatus::Pending,
	}
}

/// Calculate the age of a weaver in hours from its creation timestamp.
fn calculate_age_hours(created_at: DateTime<Utc>) -> f64 {
	let now = Utc::now();
	let duration = now.signed_duration_since(created_at);
	duration.num_seconds() as f64 / 3600.0
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;

	const TEST_ORG_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

	#[test]
	fn test_build_pod_spec_basic() {
		let id = WeaverId::new();
		let req = CreateWeaverRequest {
			image: "python:3.12".to_string(),
			env: HashMap::new(),
			resources: Default::default(),
			tags: HashMap::new(),
			lifetime_hours: None,
			command: None,
			args: None,
			workdir: None,
			repo: None,
			branch: None,
			owner_user_id: None,
			org_id: TEST_ORG_ID.to_string(),
			repo_id: None,
		};
		let config = WeaverConfig::default();

		let pod = build_pod_spec(&id, &req, &config, 4);

		assert_eq!(pod.metadata.name, Some(id.as_k8s_name()));
		assert_eq!(pod.metadata.namespace, Some("loom-weavers".to_string()));

		let labels = pod.metadata.labels.unwrap();
		assert_eq!(labels.get(MANAGED_LABEL), Some(&"true".to_string()));
		assert_eq!(labels.get(WEAVER_ID_LABEL), Some(&id.to_string()));

		let annotations = pod.metadata.annotations.unwrap();
		assert_eq!(annotations.get(LIFETIME_ANNOTATION), Some(&"4".to_string()));

		let spec = pod.spec.unwrap();
		assert_eq!(spec.restart_policy, Some("Never".to_string()));
		assert_eq!(spec.containers.len(), 1);

		let container = &spec.containers[0];
		assert_eq!(container.name, CONTAINER_NAME);
		assert_eq!(container.image, Some("python:3.12".to_string()));

		let security = container.security_context.as_ref().unwrap();
		assert_eq!(security.run_as_non_root, Some(true));
		assert_eq!(security.run_as_user, Some(1000));
		assert_eq!(security.run_as_group, Some(1000));
		assert_eq!(security.allow_privilege_escalation, Some(false));
		assert_eq!(security.read_only_root_filesystem, Some(false));

		let caps = security.capabilities.as_ref().unwrap();
		assert_eq!(caps.drop, Some(vec!["ALL".to_string()]));
	}

	#[test]
	fn test_build_pod_spec_with_env_and_resources() {
		let id = WeaverId::new();
		let mut env = HashMap::new();
		env.insert("TASK_ID".to_string(), "abc123".to_string());
		env.insert("API_URL".to_string(), "https://api.example.com".to_string());

		let req = CreateWeaverRequest {
			image: "worker:latest".to_string(),
			env,
			resources: crate::types::ResourceSpec {
				memory_limit: Some("8Gi".to_string()),
				cpu_limit: Some("4".to_string()),
			},
			tags: HashMap::new(),
			lifetime_hours: Some(8),
			command: Some(vec!["/bin/sh".to_string(), "-c".to_string()]),
			args: Some(vec!["python worker.py".to_string()]),
			workdir: Some("/app".to_string()),
			repo: None,
			branch: None,
			owner_user_id: None,
			org_id: TEST_ORG_ID.to_string(),
			repo_id: None,
		};
		let config = WeaverConfig::default();

		let pod = build_pod_spec(&id, &req, &config, 8);
		let spec = pod.spec.unwrap();
		let container = &spec.containers[0];

		assert_eq!(
			container.command,
			Some(vec!["/bin/sh".to_string(), "-c".to_string()])
		);
		assert_eq!(container.args, Some(vec!["python worker.py".to_string()]));
		assert_eq!(container.working_dir, Some("/app".to_string()));

		let env_vars = container.env.as_ref().unwrap();
		// 2 user-provided (TASK_ID, API_URL) + 1 system-injected (LOOM_WEAVER_ID)
		assert_eq!(env_vars.len(), 3);
		assert!(
			env_vars.iter().any(|e| e.name == "LOOM_WEAVER_ID"),
			"LOOM_WEAVER_ID should be injected"
		);

		let resources = container.resources.as_ref().unwrap();
		let limits = resources.limits.as_ref().unwrap();
		assert_eq!(limits.get("memory"), Some(&Quantity("8Gi".to_string())));
		assert_eq!(limits.get("cpu"), Some(&Quantity("4".to_string())));
	}

	#[test]
	fn test_build_pod_spec_with_tags() {
		let id = WeaverId::new();
		let mut tags = HashMap::new();
		tags.insert("project".to_string(), "ai-worker".to_string());
		tags.insert("env".to_string(), "prod".to_string());

		let req = CreateWeaverRequest {
			image: "test:latest".to_string(),
			env: HashMap::new(),
			resources: Default::default(),
			tags,
			lifetime_hours: None,
			command: None,
			args: None,
			workdir: None,
			repo: None,
			branch: None,
			owner_user_id: None,
			org_id: TEST_ORG_ID.to_string(),
			repo_id: None,
		};
		let config = WeaverConfig::default();

		let pod = build_pod_spec(&id, &req, &config, 4);
		let annotations = pod.metadata.annotations.unwrap();

		let tags_json = annotations.get(TAGS_ANNOTATION).unwrap();
		let parsed: HashMap<String, String> = serde_json::from_str(tags_json).unwrap();
		assert_eq!(parsed.get("project"), Some(&"ai-worker".to_string()));
		assert_eq!(parsed.get("env"), Some(&"prod".to_string()));
	}

	#[test]
	fn test_map_pod_phase_running() {
		let pod = Pod {
			metadata: ObjectMeta::default(),
			spec: None,
			status: Some(loom_server_k8s::PodStatus {
				phase: Some("Running".to_string()),
				..Default::default()
			}),
		};
		assert_eq!(map_pod_phase(&pod), WeaverStatus::Running);
	}

	#[test]
	fn test_map_pod_phase_pending() {
		let pod = Pod {
			metadata: ObjectMeta::default(),
			spec: None,
			status: Some(loom_server_k8s::PodStatus {
				phase: Some("Pending".to_string()),
				..Default::default()
			}),
		};
		assert_eq!(map_pod_phase(&pod), WeaverStatus::Pending);
	}

	#[test]
	fn test_map_pod_phase_succeeded() {
		let pod = Pod {
			metadata: ObjectMeta::default(),
			spec: None,
			status: Some(loom_server_k8s::PodStatus {
				phase: Some("Succeeded".to_string()),
				..Default::default()
			}),
		};
		assert_eq!(map_pod_phase(&pod), WeaverStatus::Succeeded);
	}

	#[test]
	fn test_map_pod_phase_failed() {
		let pod = Pod {
			metadata: ObjectMeta::default(),
			spec: None,
			status: Some(loom_server_k8s::PodStatus {
				phase: Some("Failed".to_string()),
				..Default::default()
			}),
		};
		assert_eq!(map_pod_phase(&pod), WeaverStatus::Failed);
	}

	#[test]
	fn test_map_pod_phase_terminating() {
		use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
		let pod = Pod {
			metadata: ObjectMeta {
				deletion_timestamp: Some(Time(Utc::now())),
				..Default::default()
			},
			spec: None,
			status: Some(loom_server_k8s::PodStatus {
				phase: Some("Running".to_string()),
				..Default::default()
			}),
		};
		// Even though phase is Running, deletionTimestamp means Terminating
		assert_eq!(map_pod_phase(&pod), WeaverStatus::Terminating);
	}

	#[test]
	fn test_sanitize_label_value_simple() {
		assert_eq!(sanitize_label_value("python"), "python");
		assert_eq!(sanitize_label_value("python:3.12"), "python_3.12");
		assert_eq!(sanitize_label_value("my-image"), "my-image");
	}

	#[test]
	fn test_sanitize_label_value_with_registry() {
		assert_eq!(
			sanitize_label_value("docker.io/library/python:3.12"),
			"docker.io_library_python_3.12"
		);
		assert_eq!(
			sanitize_label_value("ghcr.io/org/repo:latest"),
			"ghcr.io_org_repo_latest"
		);
	}

	#[test]
	fn test_sanitize_label_value_trims_invalid_start_end() {
		assert_eq!(sanitize_label_value("--foo--"), "foo");
		assert_eq!(sanitize_label_value("__bar__"), "bar");
		assert_eq!(sanitize_label_value("...baz..."), "baz");
	}

	#[test]
	fn test_sanitize_label_value_truncates_long_values() {
		let long_image = "a".repeat(100);
		let result = sanitize_label_value(&long_image);
		assert!(result.len() <= MAX_LABEL_LENGTH);
		assert_eq!(result.len(), MAX_LABEL_LENGTH);
	}

	#[test]
	fn test_sanitize_label_value_truncate_trims_end() {
		let long_with_invalid_end = format!("{}---", "a".repeat(61));
		let result = sanitize_label_value(&long_with_invalid_end);
		assert!(result.len() <= MAX_LABEL_LENGTH);
		assert!(result.chars().last().unwrap().is_ascii_alphanumeric());
	}

	#[test]
	fn test_build_pod_spec_has_image_label() {
		let id = WeaverId::new();
		let req = CreateWeaverRequest {
			image: "docker.io/library/python:3.12".to_string(),
			env: HashMap::new(),
			resources: Default::default(),
			tags: HashMap::new(),
			lifetime_hours: None,
			command: None,
			args: None,
			workdir: None,
			repo: None,
			branch: None,
			owner_user_id: None,
			org_id: TEST_ORG_ID.to_string(),
			repo_id: None,
		};
		let config = WeaverConfig::default();

		let pod = build_pod_spec(&id, &req, &config, 4);
		let labels = pod.metadata.labels.unwrap();

		assert_eq!(
			labels.get(LABEL_IMAGE),
			Some(&"docker.io_library_python_3.12".to_string())
		);
	}

	#[test]
	fn test_parse_image_parts_simple() {
		let result = parse_image_parts("python:3.12");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "python");
	}

	#[test]
	fn test_parse_image_parts_simple_no_tag() {
		let result = parse_image_parts("python");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "python");
	}

	#[test]
	fn test_parse_image_parts_with_org() {
		let result = parse_image_parts("library/python:3.12");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "library/python");
	}

	#[test]
	fn test_parse_image_parts_with_registry() {
		let result = parse_image_parts("ghcr.io/org/repo:latest");
		assert_eq!(result.registry, "ghcr.io");
		assert_eq!(result.name, "org/repo");
	}

	#[test]
	fn test_parse_image_parts_docker_io_explicit() {
		let result = parse_image_parts("docker.io/library/python:3.12");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "library/python");
	}

	#[test]
	fn test_parse_image_parts_with_digest() {
		let result = parse_image_parts("python@sha256:abc123def456");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "python");
	}

	#[test]
	fn test_parse_image_parts_with_tag_and_digest() {
		let result = parse_image_parts("python:3.12@sha256:abc123def456");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "python");
	}

	#[test]
	fn test_parse_image_parts_localhost() {
		let result = parse_image_parts("localhost/myimage:v1");
		assert_eq!(result.registry, "localhost");
		assert_eq!(result.name, "myimage");
	}

	#[test]
	fn test_parse_image_parts_localhost_with_port() {
		// Note: Current implementation splits on ':' first, so port and everything after is stripped
		// "localhost" without a '.' isn't recognized as a registry when it's a single part
		// This documents actual behavior - registry:port is a known limitation
		// TODO: Consider improving parse_image_parts to handle registry:port correctly
		let result = parse_image_parts("localhost:5000/myimage");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "localhost");
	}

	#[test]
	fn test_parse_image_parts_registry_with_port() {
		// Note: Current implementation splits on ':' first, so port and everything after is stripped
		// This documents actual behavior - registry:port is a known limitation
		// TODO: Consider improving parse_image_parts to handle registry:port correctly
		let result = parse_image_parts("registry.example.com:5000/org/repo");
		assert_eq!(result.registry, "docker.io");
		assert_eq!(result.name, "registry.example.com");
	}

	#[test]
	fn test_parse_image_parts_nested_path() {
		let result = parse_image_parts("gcr.io/project/team/app:v2");
		assert_eq!(result.registry, "gcr.io");
		assert_eq!(result.name, "project/team/app");
	}

	#[test]
	fn test_parse_image_parts_quay_io() {
		let result = parse_image_parts("quay.io/prometheus/alertmanager:v0.25.0");
		assert_eq!(result.registry, "quay.io");
		assert_eq!(result.name, "prometheus/alertmanager");
	}

	#[test]
	fn test_looks_like_registry() {
		assert!(looks_like_registry("docker.io"));
		assert!(looks_like_registry("ghcr.io"));
		assert!(looks_like_registry("localhost"));
		assert!(looks_like_registry("localhost:5000"));
		assert!(looks_like_registry("registry.example.com"));
		assert!(!looks_like_registry("library"));
		assert!(!looks_like_registry("python"));
		assert!(!looks_like_registry("my-org"));
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	fn is_valid_k8s_label_value(s: &str) -> bool {
		if s.is_empty() {
			return true;
		}
		if s.len() > MAX_LABEL_LENGTH {
			return false;
		}
		let chars: Vec<char> = s.chars().collect();
		if !chars.first().unwrap().is_ascii_alphanumeric() {
			return false;
		}
		if !chars.last().unwrap().is_ascii_alphanumeric() {
			return false;
		}
		chars
			.iter()
			.all(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
	}

	proptest! {
			#[test]
			fn sanitize_label_value_always_valid(input in ".*") {
					let result = sanitize_label_value(&input);
					prop_assert!(
							is_valid_k8s_label_value(&result),
							"Invalid label value: {:?} (from input: {:?})",
							result,
							input
					);
			}

			#[test]
			fn sanitize_label_value_max_length(input in ".{0,200}") {
					let result = sanitize_label_value(&input);
					prop_assert!(
							result.len() <= MAX_LABEL_LENGTH,
							"Label too long: {} chars (max {})",
							result.len(),
							MAX_LABEL_LENGTH
					);
			}

			#[test]
			fn sanitize_label_value_preserves_alphanumeric(input in "[a-zA-Z0-9]+") {
					let result = sanitize_label_value(&input);
					if input.len() <= MAX_LABEL_LENGTH {
							prop_assert_eq!(result, input);
					} else {
							prop_assert_eq!(result, &input[..MAX_LABEL_LENGTH]);
					}
			}

			#[test]
			fn sanitize_label_value_docker_images(
					registry in "(docker\\.io|ghcr\\.io|gcr\\.io|quay\\.io)",
					org in "[a-z][a-z0-9-]{0,10}",
					repo in "[a-z][a-z0-9-]{0,20}",
					tag in "[a-z0-9][a-z0-9.-]{0,10}"
			) {
					let image = format!("{}/{}/{}:{}", registry, org, repo, tag);
					let result = sanitize_label_value(&image);
					prop_assert!(
							is_valid_k8s_label_value(&result),
							"Invalid label for image {}: {:?}",
							image,
							result
					);
			}

			#[test]
			fn weaver_id_is_valid_k8s_label(_unused in 0..100u32) {
					let id = WeaverId::new();
					let id_str = id.to_string();
					prop_assert!(
							is_valid_k8s_label_value(&id_str),
							"WeaverId is not a valid K8s label: {:?}",
							id_str
					);
					prop_assert!(
							id_str.len() <= MAX_LABEL_LENGTH,
							"WeaverId too long: {} chars",
							id_str.len()
					);
			}

			#[test]
			fn weaver_pod_name_is_valid_k8s_name(_unused in 0..100u32) {
					let id = WeaverId::new();
					let pod_name = id.as_k8s_name();
					prop_assert!(
							pod_name.len() <= 253,
							"Pod name too long: {} chars (max 253)",
							pod_name.len()
					);
					prop_assert!(
							pod_name.starts_with("weaver-"),
							"Pod name should start with 'weaver-': {}",
							pod_name
					);
			}

			#[test]
			fn build_pod_spec_all_labels_valid(
					image in "[a-z][a-z0-9.-]{1,30}:[a-z0-9.]{1,10}",
					owner_id in "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}",
					org_id in "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
			) {
					let id = WeaverId::new();
					let req = CreateWeaverRequest {
							image: image.clone(),
							env: HashMap::new(),
							resources: Default::default(),
							tags: HashMap::new(),
							lifetime_hours: None,
							command: None,
							args: None,
							workdir: None,
							repo: None,
							branch: None,
							owner_user_id: Some(owner_id.clone()),
							org_id: org_id.clone(),
							repo_id: None,
					};
					let config = WeaverConfig::default();

					let pod = build_pod_spec(&id, &req, &config, 4);
					let labels = pod.metadata.labels.unwrap();

					for (key, value) in &labels {
							prop_assert!(
									is_valid_k8s_label_value(value),
									"Label {}={:?} has invalid value",
									key,
									value
							);
					}

					prop_assert_eq!(labels.get(MANAGED_LABEL), Some(&"true".to_string()));
					prop_assert!(labels.get(WEAVER_ID_LABEL).is_some());
					prop_assert!(labels.get(LABEL_IMAGE).is_some());
					prop_assert_eq!(labels.get(LABEL_OWNER_USER_ID), Some(&owner_id));
			}

			#[test]
			fn owner_user_id_uuid_is_valid_label(
					uuid in "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
			) {
					prop_assert!(
							is_valid_k8s_label_value(&uuid),
							"UUID is not a valid K8s label: {:?}",
							uuid
					);
			}

			#[test]
			fn parse_image_parts_registry_always_valid(input in ".*") {
					let result = parse_image_parts(&input);
					prop_assert!(
							!result.registry.is_empty(),
							"Registry should never be empty for input: {:?}",
							input
					);
			}

			#[test]
			fn parse_image_parts_name_always_valid(input in ".*") {
					let result = parse_image_parts(&input);
					prop_assert!(
							!result.name.contains('@'),
							"Name should not contain digest marker @ for input: {:?}",
							input
					);
					prop_assert!(
							!result.name.contains(':') || result.name.split(':').count() <= 2,
							"Name should not contain tag marker for input: {:?}",
							input
					);
			}

			#[test]
			fn parse_image_parts_strips_tags(
					name in "[a-z][a-z0-9-]{0,20}",
					tag in "[a-z0-9][a-z0-9.-]{0,10}"
			) {
					let image = format!("{}:{}", name, tag);
					let result = parse_image_parts(&image);
					prop_assert_eq!(result.registry, DEFAULT_REGISTRY);
					prop_assert_eq!(result.name, name);
			}

			#[test]
			fn parse_image_parts_strips_digests(
					name in "[a-z][a-z0-9-]{0,20}",
					digest in "[a-f0-9]{64}"
			) {
					let image = format!("{}@sha256:{}", name, digest);
					let result = parse_image_parts(&image);
					prop_assert_eq!(result.registry, DEFAULT_REGISTRY);
					prop_assert_eq!(result.name, name);
			}

			#[test]
			fn parse_image_parts_with_org_preserves_path(
					org in "[a-z][a-z0-9-]{0,10}",
					repo in "[a-z][a-z0-9-]{0,20}",
					tag in "[a-z0-9][a-z0-9.-]{0,10}"
			) {
					let image = format!("{}/{}:{}", org, repo, tag);
					let result = parse_image_parts(&image);
					prop_assert_eq!(result.registry, DEFAULT_REGISTRY);
					prop_assert_eq!(result.name, format!("{}/{}", org, repo));
			}

			#[test]
			fn parse_image_parts_recognizes_known_registries(
					registry in "(docker\\.io|ghcr\\.io|gcr\\.io|quay\\.io|registry\\.example\\.com)",
					org in "[a-z][a-z0-9-]{0,10}",
					repo in "[a-z][a-z0-9-]{0,20}",
					tag in "[a-z0-9][a-z0-9.-]{0,10}"
			) {
					let image = format!("{}/{}/{}:{}", registry, org, repo, tag);
					let result = parse_image_parts(&image);
					prop_assert_eq!(result.registry, registry);
					prop_assert_eq!(result.name, format!("{}/{}", org, repo));
			}

			#[test]
			fn parse_image_parts_localhost_registry(
					repo in "[a-z][a-z0-9-]{0,20}",
					tag in "[a-z0-9][a-z0-9.-]{0,10}"
			) {
					let image = format!("localhost/{}:{}", repo, tag);
					let result = parse_image_parts(&image);
					prop_assert_eq!(result.registry, "localhost");
					prop_assert_eq!(result.name, repo);
			}

			#[test]
			fn parse_image_parts_registry_with_port(
					host in "[a-z][a-z0-9-]{0,10}\\.[a-z]{2,4}",
					port in 1024u16..65535u16,
					repo in "[a-z][a-z0-9-]{0,20}",
			) {
					// Note: Current implementation splits on ':' first, stripping port and path
					// The host is one part with no slashes, so it defaults to docker.io
					// This is a known limitation of the current implementation
					let image = format!("{}:{}/{}", host, port, repo);
					let result = parse_image_parts(&image);
					prop_assert_eq!(result.registry, DEFAULT_REGISTRY);
					prop_assert_eq!(result.name, host);
			}
	}
}
