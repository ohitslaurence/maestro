// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod endpoint;
pub mod error;
pub mod magic_conn;
pub mod stun;
pub mod upgrade;

pub use endpoint::{select_best_endpoint, DiscoveredEndpoint, EndpointSource};
pub use error::{ConnError, Result};
pub use magic_conn::{MagicConn, PathType, PeerEndpoint};
pub use stun::{
	build_binding_request, discover_endpoint, parse_binding_response, resolve_stun_servers,
	StunError, DEFAULT_STUN_SERVERS,
};
pub use upgrade::{probe_direct, DIRECT_STALE_TIMEOUT, UPGRADE_INTERVAL};
