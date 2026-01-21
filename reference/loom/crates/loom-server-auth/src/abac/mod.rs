// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Attribute-Based Access Control (ABAC) engine.
//!
//! This module provides a policy-based authorization system that evaluates
//! access decisions based on subject attributes (who), resource attributes
//! (what), and the requested action (how).
//!
//! # Architecture
//!
//! The ABAC system is structured in three layers:
//!
//! 1. **Types** ([`types`]): Core data structures for subjects, resources, and actions
//! 2. **Policies** ([`policies`]): Resource-specific policy evaluators (thread, org, llm)
//! 3. **Engine** ([`engine`]): Main entry point that routes to appropriate policies
//!
//! # Policy Evaluation Flow
//!
//! ```text
//! is_allowed(subject, action, resource)
//!     │
//!     ├── Check global roles (SystemAdmin → always allowed)
//!     │                      (Auditor → read-only allowed)
//!     │
//!     └── Route to resource-specific policy:
//!         ├── Thread → thread::evaluate()
//!         ├── Organization → org::evaluate_org()
//!         ├── Team → org::evaluate_team()
//!         ├── ApiKey → org::evaluate_api_key()
//!         ├── Llm → llm::evaluate_llm()
//!         └── Tool → llm::evaluate_tool()
//! ```
//!
//! # Example
//!
//! ```
//! use loom_server_auth::abac::{is_allowed, Action, ResourceAttrs, SubjectAttrs};
//! use loom_server_auth::{UserId, OrgId, OrgRole};
//!
//! // Create a subject (the user making the request)
//! let user_id = UserId::generate();
//! let subject = SubjectAttrs::new(user_id);
//!
//! // Create a resource (what they're accessing)
//! let resource = ResourceAttrs::thread(user_id); // User owns this thread
//!
//! // Check permission
//! assert!(is_allowed(&subject, Action::Read, &resource));
//! assert!(is_allowed(&subject, Action::Delete, &resource)); // Owner can delete
//! ```

pub mod engine;
pub mod policies;
pub mod types;

pub use engine::*;
pub use types::*;
