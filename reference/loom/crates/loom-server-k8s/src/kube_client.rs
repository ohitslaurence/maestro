// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use futures::StreamExt;
use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewSpec, TokenReviewStatus};
use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::{
	api::{Api, AttachParams, DeleteParams, ListParams, LogParams, PostParams},
	Client,
};
use std::collections::HashMap;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, instrument};

use crate::client::K8sClient;
use crate::error::K8sError;
use crate::token_review::TokenReviewResult;
use crate::types::{AttachedProcess, LogOptions, LogStream};

/// Production K8s client implementation using the kube crate.
pub struct KubeClient {
	client: Client,
}

impl KubeClient {
	/// Create a new KubeClient that auto-discovers cluster configuration.
	///
	/// This will attempt to load config from:
	/// 1. In-cluster service account (when running in K8s)
	/// 2. KUBECONFIG environment variable
	/// 3. ~/.kube/config
	pub async fn new() -> Result<Self, K8sError> {
		let client = Client::try_default().await?;
		debug!("K8s client initialized");
		Ok(Self { client })
	}
}

#[async_trait]
impl K8sClient for KubeClient {
	async fn create_pod(&self, namespace: &str, pod: Pod) -> Result<Pod, K8sError> {
		let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
		let pod = pods.create(&PostParams::default(), &pod).await?;
		Ok(pod)
	}

	async fn delete_pod(
		&self,
		name: &str,
		namespace: &str,
		grace_period_seconds: u32,
	) -> Result<(), K8sError> {
		let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
		let dp = DeleteParams {
			grace_period_seconds: Some(grace_period_seconds),
			..Default::default()
		};
		match pods.delete(name, &dp).await {
			Ok(_) => Ok(()),
			Err(kube::Error::Api(err)) if err.code == 404 => {
				Err(K8sError::PodNotFound { name: name.into() })
			}
			Err(e) => Err(e.into()),
		}
	}

	async fn list_pods(&self, namespace: &str, label_selector: &str) -> Result<Vec<Pod>, K8sError> {
		let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
		let lp = ListParams::default().labels(label_selector);
		let pod_list = pods.list(&lp).await?;
		Ok(pod_list.items)
	}

	async fn get_pod(&self, name: &str, namespace: &str) -> Result<Pod, K8sError> {
		let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
		match pods.get(name).await {
			Ok(pod) => Ok(pod),
			Err(kube::Error::Api(err)) if err.code == 404 => {
				Err(K8sError::PodNotFound { name: name.into() })
			}
			Err(e) => Err(e.into()),
		}
	}

	async fn get_namespace(&self, name: &str) -> Result<Namespace, K8sError> {
		let namespaces: Api<Namespace> = Api::all(self.client.clone());
		match namespaces.get(name).await {
			Ok(ns) => Ok(ns),
			Err(kube::Error::Api(err)) if err.code == 404 => {
				Err(K8sError::NamespaceNotFound { name: name.into() })
			}
			Err(e) => Err(e.into()),
		}
	}

	async fn stream_logs(
		&self,
		name: &str,
		namespace: &str,
		container: &str,
		opts: LogOptions,
	) -> Result<LogStream, K8sError> {
		let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
		let lp = LogParams {
			container: Some(container.to_string()),
			follow: true,
			tail_lines: Some(opts.tail.into()),
			timestamps: opts.timestamps,
			..Default::default()
		};

		let stream = pods.log_stream(name, &lp).await.map_err(|e| match e {
			kube::Error::Api(ref err) if err.code == 404 => K8sError::PodNotFound { name: name.into() },
			_ => K8sError::StreamError {
				message: e.to_string(),
			},
		})?;

		let compat_stream = stream.compat();
		let lines_stream = tokio_util::io::ReaderStream::new(compat_stream);
		let mapped = lines_stream.map(|result| result.map_err(std::io::Error::other));
		Ok(Box::pin(mapped))
	}

	async fn exec_attach(
		&self,
		name: &str,
		namespace: &str,
		container: &str,
	) -> Result<AttachedProcess, K8sError> {
		let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
		let ap = AttachParams {
			container: Some(container.to_string()),
			stdin: true,
			stdout: true,
			stderr: false,
			tty: true,
			..Default::default()
		};

		let mut attached = pods.attach(name, &ap).await.map_err(|e| match e {
			kube::Error::Api(ref err) if err.code == 404 => K8sError::PodNotFound { name: name.into() },
			_ => K8sError::AttachError {
				message: e.to_string(),
			},
		})?;

		let stdin = attached.stdin().ok_or_else(|| K8sError::AttachError {
			message: "stdin not available".into(),
		})?;
		let stdout = attached.stdout().ok_or_else(|| K8sError::AttachError {
			message: "stdout not available".into(),
		})?;

		Ok(AttachedProcess {
			stdin: Box::pin(stdin),
			stdout: Box::pin(stdout),
		})
	}

	#[instrument(skip(self, token), fields(audiences = ?audiences))]
	async fn validate_token(
		&self,
		token: &str,
		audiences: &[&str],
	) -> Result<TokenReviewResult, K8sError> {
		let token_review = TokenReview {
			metadata: Default::default(),
			spec: TokenReviewSpec {
				audiences: if audiences.is_empty() {
					None
				} else {
					Some(audiences.iter().map(|s| s.to_string()).collect())
				},
				token: Some(token.to_string()),
			},
			status: None,
		};

		let token_reviews: Api<TokenReview> = Api::all(self.client.clone());
		let response = token_reviews
			.create(&PostParams::default(), &token_review)
			.await
			.map_err(|e| K8sError::TokenReviewError {
				message: e.to_string(),
			})?;

		let status = response.status.unwrap_or(TokenReviewStatus {
			audiences: None,
			authenticated: Some(false),
			error: Some("No status in TokenReview response".to_string()),
			user: None,
		});

		if status.authenticated != Some(true) {
			debug!(error = ?status.error, "Token authentication failed");
			return Ok(TokenReviewResult::unauthenticated(status.error));
		}

		let user_info = status.user.unwrap_or_default();
		let username = user_info.username.unwrap_or_default();
		let groups = user_info.groups.unwrap_or_default();
		let audiences = status.audiences.unwrap_or_default();

		let extra: HashMap<String, Vec<String>> =
			user_info.extra.unwrap_or_default().into_iter().collect();

		debug!(
			username = %username,
			groups = ?groups,
			"Token validated successfully"
		);

		Ok(TokenReviewResult::authenticated(
			username, groups, extra, audiences,
		))
	}
}
