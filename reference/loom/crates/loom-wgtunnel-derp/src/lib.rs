// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod client;
pub mod error;
pub mod map;
pub mod protocol;

pub use client::DerpClient;
pub use error::{DerpError, Result};
pub use map::{
	apply_overlay, fetch_default_derp_map, fetch_derp_map, load_overlay_file, DerpMap, DerpNode,
	DerpOverlay, DerpRegion, DEFAULT_DERP_MAP_URL,
};
pub use protocol::{
	decode_frame_header, encode_frame_header, ClientInfoPayload, DerpFrame, ServerInfoPayload,
};
