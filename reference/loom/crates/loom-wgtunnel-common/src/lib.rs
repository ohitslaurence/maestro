// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod derp_map;
pub mod ip;
pub mod keys;
pub mod keys_file;
pub mod peer;
pub mod session;

pub use derp_map::{
	apply_overlay, fetch_default_derp_map, fetch_derp_map, load_overlay_file, DerpMap, DerpNode,
	DerpOverlay, DerpRegion, DEFAULT_DERP_MAP_URL,
};
pub use ip::{
	is_client_ip, is_weaver_ip, parse_ipv6, server_ip, IpAllocator, CLIENT_SUBNET, NETWORK_PREFIX,
	SERVER_IP, WEAVER_SUBNET,
};
pub use keys::{KeyError, WgKeyPair, WgPrivateKey, WgPublicKey};
pub use keys_file::{
	default_config_dir, get_or_create_device_key, load_wg_key_env, load_wg_key_from_file,
	save_wg_key_to_file,
};
pub use peer::{PeerId, PeerInfo};
pub use session::{DeviceId, SessionId, SessionInfo, WeaverId};
