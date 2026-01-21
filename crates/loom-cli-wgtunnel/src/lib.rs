// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod client;
pub mod commands;
pub mod error;
pub mod keys;
pub mod tunnel;

pub use client::{CreateSessionResponse, DeviceInfo, SessionInfo, WeaverInfo, WgTunnelClient};
pub use commands::devices::{
	handle_list as handle_devices_list, handle_register as handle_devices_register,
	handle_revoke as handle_devices_revoke, DevicesCommands, RegisterArgs, RevokeArgs,
};
pub use commands::ssh::{handle_ssh, SshArgs};
pub use commands::tunnel::{
	handle_down as handle_tunnel_down, handle_status as handle_tunnel_status,
	handle_up as handle_tunnel_up, CliContext, TunnelCommands, TunnelUpArgs,
};
pub use error::{CliError, Result};
pub use keys::{get_or_create_device_key, key_exists, load_key_file, save_key_file, KEY_FILENAME};
pub use tunnel::{TunnelConfig, TunnelManager, TunnelStatus, WeaverConnectionStatus};
