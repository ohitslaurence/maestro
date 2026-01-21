// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

pub use loom_server_db::{BranchProtectionRuleRecord, ProtectionRepository, ProtectionStore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectionViolation {
	DirectPushBlocked { branch: String, pattern: String },
	ForcePushBlocked { branch: String, pattern: String },
	DeletionBlocked { branch: String, pattern: String },
}

impl std::fmt::Display for ProtectionViolation {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ProtectionViolation::DirectPushBlocked { branch, pattern } => {
				write!(
					f,
					"Direct push to branch '{}' is blocked by protection rule '{}'",
					branch, pattern
				)
			}
			ProtectionViolation::ForcePushBlocked { branch, pattern } => {
				write!(
					f,
					"Force push to branch '{}' is blocked by protection rule '{}'",
					branch, pattern
				)
			}
			ProtectionViolation::DeletionBlocked { branch, pattern } => {
				write!(
					f,
					"Deletion of branch '{}' is blocked by protection rule '{}'",
					branch, pattern
				)
			}
		}
	}
}

#[derive(Debug, Clone)]
pub struct PushCheck {
	pub branch: String,
	pub is_force_push: bool,
	pub is_deletion: bool,
	pub user_is_admin: bool,
}

pub fn matches_pattern(pattern: &str, branch: &str) -> bool {
	if pattern == branch {
		return true;
	}

	if let Some(prefix) = pattern.strip_suffix("/*") {
		return branch.starts_with(&format!("{}/", prefix));
	}

	if let Some(prefix) = pattern.strip_suffix('*') {
		return branch.starts_with(prefix);
	}

	false
}

pub fn check_push_allowed(
	rules: &[BranchProtectionRuleRecord],
	check: &PushCheck,
) -> std::result::Result<(), ProtectionViolation> {
	if check.user_is_admin {
		return Ok(());
	}

	for rule in rules {
		if !matches_pattern(&rule.pattern, &check.branch) {
			continue;
		}

		if check.is_deletion && rule.block_deletion {
			return Err(ProtectionViolation::DeletionBlocked {
				branch: check.branch.clone(),
				pattern: rule.pattern.clone(),
			});
		}

		if check.is_force_push && rule.block_force_push {
			return Err(ProtectionViolation::ForcePushBlocked {
				branch: check.branch.clone(),
				pattern: rule.pattern.clone(),
			});
		}

		if rule.block_direct_push {
			return Err(ProtectionViolation::DirectPushBlocked {
				branch: check.branch.clone(),
				pattern: rule.pattern.clone(),
			});
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;
	use uuid::Uuid;

	fn make_rule(repo_id: Uuid, pattern: &str) -> BranchProtectionRuleRecord {
		BranchProtectionRuleRecord {
			id: Uuid::new_v4(),
			repo_id,
			pattern: pattern.to_string(),
			block_direct_push: true,
			block_force_push: true,
			block_deletion: true,
			created_at: Utc::now(),
		}
	}

	#[test]
	fn test_matches_pattern_exact() {
		assert!(matches_pattern("cannon", "cannon"));
		assert!(!matches_pattern("cannon", "main"));
		assert!(!matches_pattern("cannon", "cannons"));
	}

	#[test]
	fn test_matches_pattern_wildcard() {
		assert!(matches_pattern("release/*", "release/v1.0"));
		assert!(matches_pattern("release/*", "release/"));
		assert!(!matches_pattern("release/*", "release"));
		assert!(!matches_pattern("release/*", "releases/v1.0"));
	}

	#[test]
	fn test_matches_pattern_prefix_wildcard() {
		assert!(matches_pattern("feat*", "feature"));
		assert!(matches_pattern("feat*", "feat"));
		assert!(matches_pattern("feat*", "feat-new"));
		assert!(!matches_pattern("feat*", "fix"));
	}

	#[test]
	fn test_check_push_allowed_admin_bypass() {
		let rules = vec![make_rule(Uuid::new_v4(), "cannon")];
		let check = PushCheck {
			branch: "cannon".to_string(),
			is_force_push: true,
			is_deletion: true,
			user_is_admin: true,
		};
		assert!(check_push_allowed(&rules, &check).is_ok());
	}

	#[test]
	fn test_check_push_allowed_direct_push_blocked() {
		let rules = vec![make_rule(Uuid::new_v4(), "cannon")];
		let check = PushCheck {
			branch: "cannon".to_string(),
			is_force_push: false,
			is_deletion: false,
			user_is_admin: false,
		};
		let result = check_push_allowed(&rules, &check);
		assert!(matches!(
			result,
			Err(ProtectionViolation::DirectPushBlocked { .. })
		));
	}

	#[test]
	fn test_check_push_allowed_force_push_blocked() {
		let mut rule = make_rule(Uuid::new_v4(), "cannon");
		rule.block_direct_push = false;
		let rules = vec![rule];
		let check = PushCheck {
			branch: "cannon".to_string(),
			is_force_push: true,
			is_deletion: false,
			user_is_admin: false,
		};
		let result = check_push_allowed(&rules, &check);
		assert!(matches!(
			result,
			Err(ProtectionViolation::ForcePushBlocked { .. })
		));
	}

	#[test]
	fn test_check_push_allowed_deletion_blocked() {
		let mut rule = make_rule(Uuid::new_v4(), "cannon");
		rule.block_direct_push = false;
		rule.block_force_push = false;
		let rules = vec![rule];
		let check = PushCheck {
			branch: "cannon".to_string(),
			is_force_push: false,
			is_deletion: true,
			user_is_admin: false,
		};
		let result = check_push_allowed(&rules, &check);
		assert!(matches!(
			result,
			Err(ProtectionViolation::DeletionBlocked { .. })
		));
	}

	#[test]
	fn test_check_push_allowed_unprotected_branch() {
		let rules = vec![make_rule(Uuid::new_v4(), "cannon")];
		let check = PushCheck {
			branch: "feature/new-thing".to_string(),
			is_force_push: true,
			is_deletion: true,
			user_is_admin: false,
		};
		assert!(check_push_allowed(&rules, &check).is_ok());
	}

	#[test]
	fn test_check_push_allowed_wildcard_pattern() {
		let rules = vec![make_rule(Uuid::new_v4(), "release/*")];
		let check = PushCheck {
			branch: "release/v1.0".to_string(),
			is_force_push: false,
			is_deletion: false,
			user_is_admin: false,
		};
		let result = check_push_allowed(&rules, &check);
		assert!(matches!(
			result,
			Err(ProtectionViolation::DirectPushBlocked { .. })
		));
	}

	#[test]
	fn test_matches_pattern_prefix_star_empty_suffix() {
		// "feat*" should match "feat" itself
		assert!(matches_pattern("feat*", "feat"));
		assert!(matches_pattern("feat*", "feat-something"));
		assert!(matches_pattern("feat*", "feature"));
	}

	#[test]
	fn test_protection_violation_display() {
		let direct = ProtectionViolation::DirectPushBlocked {
			branch: "main".to_string(),
			pattern: "main".to_string(),
		};
		assert!(direct.to_string().contains("Direct push"));
		assert!(direct.to_string().contains("main"));

		let force = ProtectionViolation::ForcePushBlocked {
			branch: "cannon".to_string(),
			pattern: "cannon".to_string(),
		};
		assert!(force.to_string().contains("Force push"));

		let deletion = ProtectionViolation::DeletionBlocked {
			branch: "release/v1".to_string(),
			pattern: "release/*".to_string(),
		};
		assert!(deletion.to_string().contains("Deletion"));
	}

	mod proptest_patterns {
		use super::*;
		use proptest::prelude::*;

		/// Property: Exact pattern always matches the exact branch name
		#[test]
		fn prop_exact_pattern_matches_self() {
			proptest!(|(branch in "[a-zA-Z][a-zA-Z0-9_-]{0,20}")| {
				prop_assert!(matches_pattern(&branch, &branch));
			});
		}

		/// Property: Exact pattern does not match different branch names
		#[test]
		fn prop_exact_pattern_no_false_positives() {
			proptest!(|(
				pattern in "[a-zA-Z][a-zA-Z0-9]{0,10}",
				suffix in "[a-zA-Z0-9]{1,5}"
			)| {
				// pattern should not match pattern + suffix (different strings)
				let branch = format!("{}{}", pattern, suffix);
				if pattern != branch {
					prop_assert!(!matches_pattern(&pattern, &branch));
				}
			});
		}

		/// Property: Wildcard pattern "prefix/*" matches "prefix/anything"
		#[test]
		fn prop_slash_wildcard_matches_subdirs() {
			proptest!(|(
				prefix in "[a-zA-Z][a-zA-Z0-9]{0,10}",
				subpath in "[a-zA-Z0-9][a-zA-Z0-9/_-]{0,15}"
			)| {
				let pattern = format!("{}/*", prefix);
				let branch = format!("{}/{}", prefix, subpath);
				prop_assert!(matches_pattern(&pattern, &branch),
					"Pattern '{}' should match '{}'", pattern, branch);
			});
		}

		/// Property: Wildcard pattern "prefix/*" does NOT match "prefix" alone
		#[test]
		fn prop_slash_wildcard_requires_slash() {
			proptest!(|(prefix in "[a-zA-Z][a-zA-Z0-9]{0,10}")| {
				let pattern = format!("{}/*", prefix);
				prop_assert!(!matches_pattern(&pattern, &prefix),
					"Pattern '{}' should NOT match '{}' (no slash)", pattern, prefix);
			});
		}

		/// Property: Prefix wildcard "prefix*" matches anything starting with prefix
		#[test]
		fn prop_prefix_wildcard_matches_extensions() {
			proptest!(|(
				prefix in "[a-zA-Z]{1,5}",
				suffix in "[a-zA-Z0-9_-]{0,10}"
			)| {
				let pattern = format!("{}*", prefix);
				let branch = format!("{}{}", prefix, suffix);
				prop_assert!(matches_pattern(&pattern, &branch),
					"Pattern '{}' should match '{}'", pattern, branch);
			});
		}

		/// Property: Admin always bypasses protection
		#[test]
		fn prop_admin_always_bypasses() {
			proptest!(|(
				pattern in "[a-zA-Z][a-zA-Z0-9]{0,10}",
				is_force in any::<bool>(),
				is_deletion in any::<bool>()
			)| {
				let rule = BranchProtectionRuleRecord {
					id: Uuid::new_v4(),
					repo_id: Uuid::new_v4(),
					pattern: pattern.clone(),
					block_direct_push: true,
					block_force_push: true,
					block_deletion: true,
					created_at: Utc::now(),
				};
				let rules = vec![rule];
				let check = PushCheck {
					branch: pattern,
					is_force_push: is_force,
					is_deletion: is_deletion,
					user_is_admin: true,
				};
				prop_assert!(check_push_allowed(&rules, &check).is_ok());
			});
		}

		/// Property: Non-admin blocked on protected branch with block_direct_push
		#[test]
		fn prop_non_admin_blocked_on_direct_push() {
			proptest!(|(pattern in "[a-zA-Z][a-zA-Z0-9]{0,10}")| {
				let rule = BranchProtectionRuleRecord {
					id: Uuid::new_v4(),
					repo_id: Uuid::new_v4(),
					pattern: pattern.clone(),
					block_direct_push: true,
					block_force_push: false,
					block_deletion: false,
					created_at: Utc::now(),
				};
				let rules = vec![rule];
				let check = PushCheck {
					branch: pattern,
					is_force_push: false,
					is_deletion: false,
					user_is_admin: false,
				};
				let result = check_push_allowed(&rules, &check);
				let is_blocked = matches!(result, Err(ProtectionViolation::DirectPushBlocked { branch: _, pattern: _ }));
				prop_assert!(is_blocked, "Expected DirectPushBlocked violation");
			});
		}

		/// Property: Unprotected branches always allowed
		#[test]
		fn prop_unprotected_branch_allowed() {
			proptest!(|(
				protected in "[a-zA-Z]{1,5}",
				unprotected in "[a-zA-Z]{1,5}",
				is_force in any::<bool>(),
				is_deletion in any::<bool>()
			)| {
				// Only run when they're different
				prop_assume!(protected != unprotected);
				prop_assume!(!unprotected.starts_with(&protected));

				let rule = BranchProtectionRuleRecord {
					id: Uuid::new_v4(),
					repo_id: Uuid::new_v4(),
					pattern: protected,
					block_direct_push: true,
					block_force_push: true,
					block_deletion: true,
					created_at: Utc::now(),
				};
				let rules = vec![rule];
				let check = PushCheck {
					branch: unprotected,
					is_force_push: is_force,
					is_deletion: is_deletion,
					user_is_admin: false,
				};
				prop_assert!(check_push_allowed(&rules, &check).is_ok());
			});
		}
	}
}
