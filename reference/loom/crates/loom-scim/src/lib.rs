// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod error;
pub mod filter;
pub mod patch;
pub mod schema;
pub mod types;

pub use error::{ScimError, ScimErrorType};
pub use filter::{evaluate_filter, Filter, FilterParser};
pub use patch::{PatchOp, PatchOperation};
pub use schema::{Schema, SchemaAttribute};
pub use types::{
	ListResponse, Meta, Name, ResourceType, ScimEmail, ScimGroup, ScimPhoneNumber, ScimResource,
	ScimUser, ServiceProviderConfig,
};
