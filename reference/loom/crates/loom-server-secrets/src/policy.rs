// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Access control policies for secrets.
//!
//! Implements ABAC-style access control for weaver secret access.

use loom_server_auth::types::OrgId;

use crate::types::{Secret, SecretScope, WeaverId};

/// Principal representing a weaver requesting secret access.
#[derive(Debug, Clone)]
pub struct WeaverPrincipal {
	/// The weaver's unique identifier.
	pub weaver_id: WeaverId,
	/// The organization the weaver belongs to.
	pub org_id: OrgId,
	/// Optional repository context for the weaver.
	pub repo_id: Option<String>,
}

impl WeaverPrincipal {
	/// Create a new weaver principal.
	pub fn new(weaver_id: WeaverId, org_id: OrgId, repo_id: Option<String>) -> Self {
		Self {
			weaver_id,
			org_id,
			repo_id,
		}
	}
}

/// Check if a weaver principal can access a secret.
///
/// Access rules:
/// - Org-scoped secrets: weaver must be in the same org
/// - Repo-scoped secrets: weaver must be in the same org and working on the same repo
/// - Weaver-scoped secrets: weaver ID must match exactly
pub fn can_access_secret(principal: &WeaverPrincipal, secret: &Secret) -> bool {
	match &secret.scope {
		SecretScope::Org { org_id } => principal.org_id == *org_id,
		SecretScope::Repo { org_id, repo_id } => {
			principal.org_id == *org_id && (principal.repo_id.as_ref() == Some(repo_id))
		}
		SecretScope::Weaver { weaver_id } => principal.weaver_id == *weaver_id,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use loom_server_auth::types::UserId;

	use crate::types::SecretId;

	fn make_secret(scope: SecretScope) -> Secret {
		Secret {
			id: SecretId::generate(),
			name: "TEST_SECRET".to_string(),
			description: None,
			scope,
			current_version: 1,
			created_by: UserId::generate(),
			created_at: Utc::now(),
			updated_by: UserId::generate(),
			updated_at: Utc::now(),
			expires_at: None,
		}
	}

	#[test]
	fn org_scope_same_org_allows_access() {
		let org_id = OrgId::generate();
		let principal = WeaverPrincipal::new(WeaverId::generate(), org_id, None);
		let secret = make_secret(SecretScope::Org { org_id });
		assert!(can_access_secret(&principal, &secret));
	}

	#[test]
	fn org_scope_different_org_denies_access() {
		let principal = WeaverPrincipal::new(WeaverId::generate(), OrgId::generate(), None);
		let secret = make_secret(SecretScope::Org {
			org_id: OrgId::generate(),
		});
		assert!(!can_access_secret(&principal, &secret));
	}

	#[test]
	fn repo_scope_matching_repo_allows_access() {
		let org_id = OrgId::generate();
		let principal = WeaverPrincipal::new(WeaverId::generate(), org_id, Some("my-repo".to_string()));
		let secret = make_secret(SecretScope::Repo {
			org_id,
			repo_id: "my-repo".to_string(),
		});
		assert!(can_access_secret(&principal, &secret));
	}

	#[test]
	fn repo_scope_different_repo_denies_access() {
		let org_id = OrgId::generate();
		let principal =
			WeaverPrincipal::new(WeaverId::generate(), org_id, Some("other-repo".to_string()));
		let secret = make_secret(SecretScope::Repo {
			org_id,
			repo_id: "my-repo".to_string(),
		});
		assert!(!can_access_secret(&principal, &secret));
	}

	#[test]
	fn repo_scope_no_repo_context_denies_access() {
		let org_id = OrgId::generate();
		let principal = WeaverPrincipal::new(WeaverId::generate(), org_id, None);
		let secret = make_secret(SecretScope::Repo {
			org_id,
			repo_id: "my-repo".to_string(),
		});
		assert!(!can_access_secret(&principal, &secret));
	}

	#[test]
	fn weaver_scope_same_weaver_allows_access() {
		let weaver_id = WeaverId::generate();
		let principal = WeaverPrincipal::new(weaver_id, OrgId::generate(), None);
		let secret = make_secret(SecretScope::Weaver { weaver_id });
		assert!(can_access_secret(&principal, &secret));
	}

	#[test]
	fn weaver_scope_different_weaver_denies_access() {
		let principal = WeaverPrincipal::new(WeaverId::generate(), OrgId::generate(), None);
		let secret = make_secret(SecretScope::Weaver {
			weaver_id: WeaverId::generate(),
		});
		assert!(!can_access_secret(&principal, &secret));
	}
}
