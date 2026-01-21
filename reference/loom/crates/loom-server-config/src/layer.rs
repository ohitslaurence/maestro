// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration layer for merging from multiple sources.

use serde::Deserialize;

use crate::sections::{
	AnalyticsConfigLayer, AuditConfigLayer, AuthConfigLayer, DatabaseConfigLayer, GeoIpConfigLayer,
	GitHubAppConfigLayer, HttpConfigLayer, JobsConfigLayer, LlmConfigLayer, LoggingConfigLayer,
	OAuthConfigLayer, PathsConfigLayer, ScimConfigLayer, SearchConfigLayer, SmtpConfigLayer,
	WeaverConfigLayer,
};

/// Server configuration layer - all fields are Option for merging.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerConfigLayer {
	#[serde(default)]
	pub http: Option<HttpConfigLayer>,
	#[serde(default)]
	pub database: Option<DatabaseConfigLayer>,
	#[serde(default)]
	pub auth: Option<AuthConfigLayer>,
	#[serde(default)]
	pub llm: Option<LlmConfigLayer>,
	#[serde(default)]
	pub weaver: Option<WeaverConfigLayer>,
	#[serde(default)]
	pub smtp: Option<SmtpConfigLayer>,
	#[serde(default)]
	pub oauth: Option<OAuthConfigLayer>,
	#[serde(default)]
	pub github_app: Option<GitHubAppConfigLayer>,
	#[serde(default)]
	pub geoip: Option<GeoIpConfigLayer>,
	#[serde(default)]
	pub jobs: Option<JobsConfigLayer>,
	#[serde(default)]
	pub search: Option<SearchConfigLayer>,
	#[serde(default)]
	pub paths: Option<PathsConfigLayer>,
	#[serde(default)]
	pub logging: Option<LoggingConfigLayer>,
	#[serde(default)]
	pub audit: Option<AuditConfigLayer>,
	#[serde(default)]
	pub scim: Option<ScimConfigLayer>,
	#[serde(default)]
	pub analytics: Option<AnalyticsConfigLayer>,
}

impl ServerConfigLayer {
	/// Merge another layer into this one. Other layer takes precedence.
	pub fn merge(&mut self, other: ServerConfigLayer) {
		merge_option(&mut self.http, other.http, HttpConfigLayer::merge);
		merge_option(
			&mut self.database,
			other.database,
			DatabaseConfigLayer::merge,
		);
		merge_option(&mut self.auth, other.auth, AuthConfigLayer::merge);
		merge_option(&mut self.llm, other.llm, LlmConfigLayer::merge);
		merge_option(&mut self.weaver, other.weaver, WeaverConfigLayer::merge);
		merge_option(&mut self.smtp, other.smtp, SmtpConfigLayer::merge);
		merge_option(&mut self.oauth, other.oauth, OAuthConfigLayer::merge);
		merge_option(
			&mut self.github_app,
			other.github_app,
			GitHubAppConfigLayer::merge,
		);
		merge_option(&mut self.geoip, other.geoip, GeoIpConfigLayer::merge);
		merge_option(&mut self.jobs, other.jobs, JobsConfigLayer::merge);
		merge_option(&mut self.search, other.search, SearchConfigLayer::merge);
		merge_option(&mut self.paths, other.paths, PathsConfigLayer::merge);
		merge_option(&mut self.logging, other.logging, LoggingConfigLayer::merge);
		merge_option(&mut self.audit, other.audit, AuditConfigLayer::merge);
		merge_option(&mut self.scim, other.scim, ScimConfigLayer::merge);
		merge_option(
			&mut self.analytics,
			other.analytics,
			AnalyticsConfigLayer::merge,
		);
	}
}

fn merge_option<T, F>(target: &mut Option<T>, source: Option<T>, merge_fn: F)
where
	F: FnOnce(&mut T, T),
{
	match (target.as_mut(), source) {
		(Some(t), Some(s)) => merge_fn(t, s),
		(None, Some(s)) => *target = Some(s),
		_ => {}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_merge_empty_layers() {
		let mut base = ServerConfigLayer::default();
		let other = ServerConfigLayer::default();
		base.merge(other);
		assert!(base.http.is_none());
	}

	#[test]
	fn test_merge_preserves_base_when_other_empty() {
		let mut base = ServerConfigLayer {
			http: Some(HttpConfigLayer {
				port: Some(9000),
				..Default::default()
			}),
			..Default::default()
		};
		let other = ServerConfigLayer::default();
		base.merge(other);
		assert_eq!(base.http.as_ref().unwrap().port, Some(9000));
	}

	#[test]
	fn test_merge_other_overwrites() {
		let mut base = ServerConfigLayer {
			http: Some(HttpConfigLayer {
				port: Some(9000),
				host: Some("127.0.0.1".to_string()),
				..Default::default()
			}),
			..Default::default()
		};
		let other = ServerConfigLayer {
			http: Some(HttpConfigLayer {
				port: Some(8080),
				..Default::default()
			}),
			..Default::default()
		};
		base.merge(other);
		assert_eq!(base.http.as_ref().unwrap().port, Some(8080));
		assert_eq!(
			base.http.as_ref().unwrap().host,
			Some("127.0.0.1".to_string())
		);
	}

	#[test]
	fn test_merge_adds_missing_sections() {
		let mut base = ServerConfigLayer {
			http: Some(HttpConfigLayer {
				port: Some(9000),
				..Default::default()
			}),
			..Default::default()
		};
		let other = ServerConfigLayer {
			database: Some(DatabaseConfigLayer {
				url: Some("postgres://localhost/db".to_string()),
			}),
			..Default::default()
		};
		base.merge(other);
		assert_eq!(base.http.as_ref().unwrap().port, Some(9000));
		assert_eq!(
			base.database.as_ref().unwrap().url,
			Some("postgres://localhost/db".to_string())
		);
	}
}
