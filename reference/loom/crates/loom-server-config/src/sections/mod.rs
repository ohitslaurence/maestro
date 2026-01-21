// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration sections for loom-server.

pub mod analytics;
pub mod audit;
pub mod auth;
pub mod database;
pub mod geoip;
pub mod github_app;
pub mod http;
pub mod jobs;
pub mod llm;
pub mod logging;
pub mod oauth;
pub mod paths;
pub mod scim;
pub mod search;
pub mod smtp;
pub mod weaver;

pub use analytics::{AnalyticsConfig, AnalyticsConfigLayer};
pub use audit::{
	AuditConfig, AuditConfigLayer, FileFormat, FileSinkConfig, FileSinkConfigLayer, HttpSinkConfig,
	HttpSinkConfigLayer, JsonStreamConfig, JsonStreamConfigLayer, QueueOverflowPolicy,
	StreamProtocol, SyslogConfig, SyslogConfigLayer, SyslogProtocol,
};
pub use auth::{AuthConfig, AuthConfigLayer};
pub use database::{DatabaseConfig, DatabaseConfigLayer};
pub use geoip::{GeoIpConfig, GeoIpConfigLayer};
pub use github_app::{GitHubAppConfig, GitHubAppConfigLayer};
pub use http::{HttpConfig, HttpConfigLayer};
pub use jobs::{JobsConfig, JobsConfigLayer};
pub use llm::{AnthropicAuthConfig, LlmConfig, LlmConfigLayer, LlmProvider};
pub use logging::{LoggingConfig, LoggingConfigLayer};
pub use oauth::{
	GitHubOAuthConfig, GitHubOAuthConfigLayer, GoogleOAuthConfig, GoogleOAuthConfigLayer,
	OAuthConfig, OAuthConfigLayer, OktaOAuthConfig, OktaOAuthConfigLayer,
};
pub use paths::{PathsConfig, PathsConfigLayer};
pub use scim::{ScimConfig, ScimConfigLayer};
pub use search::{
	GoogleCseConfig, GoogleCseConfigLayer, SearchConfig, SearchConfigLayer, SerperConfig,
	SerperConfigLayer,
};
pub use smtp::{SmtpConfig, SmtpConfigLayer, TlsMode};
pub use weaver::{WeaverConfig, WeaverConfigLayer, WebhookConfig, WebhookEvent};
