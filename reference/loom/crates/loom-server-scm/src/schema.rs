// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Database schema for SCM tables.
//!
//! Note: Migrations are now managed by loom-server in:
//! - migrations/020_scm_repos.sql (repos, branch_protection_rules, repo_team_access)
//! - migrations/021_scm_webhooks.sql (webhooks, webhook_deliveries)
