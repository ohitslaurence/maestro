// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SSE (Server-Sent Events) types for real-time flag updates.
//!
//! This module defines the event types used for streaming flag updates
//! to connected SDK clients.
//!
//! # Events
//!
//! - `init` - Full state of all flags on connect
//! - `flag.updated` - Flag or config changed
//! - `flag.archived` - Flag archived
//! - `killswitch.activated` - Kill switch activated
//! - `killswitch.deactivated` - Kill switch deactivated
//! - `heartbeat` - Keep-alive (every 30s)
//!
//! # Example
//!
//! ```
//! use loom_flags_core::sse::{FlagStreamEvent, FlagUpdatedData};
//! use loom_flags_core::VariantValue;
//! use chrono::Utc;
//!
//! // Create a flag update event
//! let event = FlagStreamEvent::FlagUpdated(FlagUpdatedData {
//!     flag_key: "feature.new_flow".to_string(),
//!     environment: "prod".to_string(),
//!     enabled: true,
//!     default_variant: "on".to_string(),
//!     default_value: VariantValue::Boolean(true),
//!     timestamp: Utc::now(),
//! });
//!
//! // Serialize for SSE
//! let json = serde_json::to_string(&event).unwrap();
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
	EnvironmentId, Flag, FlagConfig, FlagId, KillSwitch, KillSwitchId, OrgId, VariantValue,
};

/// SSE event types for flag streaming.
///
/// Each variant corresponds to a specific event type that can be sent
/// to connected clients via SSE.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "data")]
pub enum FlagStreamEvent {
	/// Initial state sent on connection.
	/// Contains all flags and their current configuration.
	#[serde(rename = "init")]
	Init(InitData),

	/// Flag or config has been updated.
	#[serde(rename = "flag.updated")]
	FlagUpdated(FlagUpdatedData),

	/// Flag has been archived.
	#[serde(rename = "flag.archived")]
	FlagArchived(FlagArchivedData),

	/// Flag has been restored from archive.
	#[serde(rename = "flag.restored")]
	FlagRestored(FlagRestoredData),

	/// Kill switch has been activated.
	#[serde(rename = "killswitch.activated")]
	KillSwitchActivated(KillSwitchActivatedData),

	/// Kill switch has been deactivated.
	#[serde(rename = "killswitch.deactivated")]
	KillSwitchDeactivated(KillSwitchDeactivatedData),

	/// Heartbeat for connection keep-alive.
	#[serde(rename = "heartbeat")]
	Heartbeat(HeartbeatData),
}

impl FlagStreamEvent {
	/// Returns the event type name as a string.
	pub fn event_type(&self) -> &'static str {
		match self {
			FlagStreamEvent::Init(_) => "init",
			FlagStreamEvent::FlagUpdated(_) => "flag.updated",
			FlagStreamEvent::FlagArchived(_) => "flag.archived",
			FlagStreamEvent::FlagRestored(_) => "flag.restored",
			FlagStreamEvent::KillSwitchActivated(_) => "killswitch.activated",
			FlagStreamEvent::KillSwitchDeactivated(_) => "killswitch.deactivated",
			FlagStreamEvent::Heartbeat(_) => "heartbeat",
		}
	}

	/// Creates a new init event with the given flags and kill switches.
	pub fn init(flags: Vec<FlagState>, kill_switches: Vec<KillSwitchState>) -> Self {
		FlagStreamEvent::Init(InitData {
			flags,
			kill_switches,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new flag updated event.
	pub fn flag_updated(
		flag_key: String,
		environment: String,
		enabled: bool,
		default_variant: String,
		default_value: VariantValue,
	) -> Self {
		FlagStreamEvent::FlagUpdated(FlagUpdatedData {
			flag_key,
			environment,
			enabled,
			default_variant,
			default_value,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new flag archived event.
	pub fn flag_archived(flag_key: String) -> Self {
		FlagStreamEvent::FlagArchived(FlagArchivedData {
			flag_key,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new flag restored event.
	pub fn flag_restored(flag_key: String, environment: String, enabled: bool) -> Self {
		FlagStreamEvent::FlagRestored(FlagRestoredData {
			flag_key,
			environment,
			enabled,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new kill switch activated event.
	pub fn kill_switch_activated(
		kill_switch_key: String,
		linked_flag_keys: Vec<String>,
		reason: String,
	) -> Self {
		FlagStreamEvent::KillSwitchActivated(KillSwitchActivatedData {
			kill_switch_key,
			linked_flag_keys,
			reason,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new kill switch deactivated event.
	pub fn kill_switch_deactivated(kill_switch_key: String, linked_flag_keys: Vec<String>) -> Self {
		FlagStreamEvent::KillSwitchDeactivated(KillSwitchDeactivatedData {
			kill_switch_key,
			linked_flag_keys,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new heartbeat event.
	pub fn heartbeat() -> Self {
		FlagStreamEvent::Heartbeat(HeartbeatData {
			timestamp: Utc::now(),
		})
	}
}

/// Initial state data sent on connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitData {
	/// All flags with their current state.
	pub flags: Vec<FlagState>,
	/// All active kill switches.
	pub kill_switches: Vec<KillSwitchState>,
	/// When the init data was generated.
	pub timestamp: DateTime<Utc>,
}

/// Compact representation of a flag's current state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlagState {
	/// The flag key.
	pub key: String,
	/// Flag ID.
	pub id: FlagId,
	/// Whether the flag is enabled in this environment.
	pub enabled: bool,
	/// The default variant name.
	pub default_variant: String,
	/// The default variant value.
	pub default_value: VariantValue,
	/// Whether the flag is archived.
	pub archived: bool,
}

impl FlagState {
	/// Create a FlagState from a Flag and FlagConfig.
	pub fn from_flag_and_config(flag: &Flag, config: Option<&FlagConfig>) -> Self {
		let default_value = flag
			.variants
			.iter()
			.find(|v| v.name == flag.default_variant)
			.map(|v| v.value.clone())
			.unwrap_or(VariantValue::Boolean(false));

		FlagState {
			key: flag.key.clone(),
			id: flag.id,
			enabled: config.map(|c| c.enabled).unwrap_or(false),
			default_variant: flag.default_variant.clone(),
			default_value,
			archived: flag.archived_at.is_some(),
		}
	}
}

/// Compact representation of a kill switch's current state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KillSwitchState {
	/// The kill switch key.
	pub key: String,
	/// Kill switch ID.
	pub id: KillSwitchId,
	/// Whether the kill switch is active.
	pub is_active: bool,
	/// Flag keys affected by this kill switch.
	pub linked_flag_keys: Vec<String>,
	/// Reason for activation (if active).
	pub activation_reason: Option<String>,
}

impl From<&KillSwitch> for KillSwitchState {
	fn from(ks: &KillSwitch) -> Self {
		KillSwitchState {
			key: ks.key.clone(),
			id: ks.id,
			is_active: ks.is_active,
			linked_flag_keys: ks.linked_flag_keys.clone(),
			activation_reason: ks.activation_reason.clone(),
		}
	}
}

/// Data for flag.updated event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlagUpdatedData {
	/// The flag key that was updated.
	pub flag_key: String,
	/// The environment where the update occurred.
	pub environment: String,
	/// Whether the flag is enabled after the update.
	pub enabled: bool,
	/// The default variant name.
	pub default_variant: String,
	/// The default variant value.
	pub default_value: VariantValue,
	/// When the update occurred.
	pub timestamp: DateTime<Utc>,
}

/// Data for flag.archived event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlagArchivedData {
	/// The flag key that was archived.
	pub flag_key: String,
	/// When the archive occurred.
	pub timestamp: DateTime<Utc>,
}

/// Data for flag.restored event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlagRestoredData {
	/// The flag key that was restored.
	pub flag_key: String,
	/// The environment.
	pub environment: String,
	/// Whether the flag is enabled after restore.
	pub enabled: bool,
	/// When the restore occurred.
	pub timestamp: DateTime<Utc>,
}

/// Data for killswitch.activated event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KillSwitchActivatedData {
	/// The kill switch key that was activated.
	pub kill_switch_key: String,
	/// Flag keys affected by this kill switch.
	pub linked_flag_keys: Vec<String>,
	/// Reason for activation.
	pub reason: String,
	/// When the activation occurred.
	pub timestamp: DateTime<Utc>,
}

/// Data for killswitch.deactivated event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KillSwitchDeactivatedData {
	/// The kill switch key that was deactivated.
	pub kill_switch_key: String,
	/// Flag keys no longer affected by this kill switch.
	pub linked_flag_keys: Vec<String>,
	/// When the deactivation occurred.
	pub timestamp: DateTime<Utc>,
}

/// Data for heartbeat event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatData {
	/// When the heartbeat was sent.
	pub timestamp: DateTime<Utc>,
}

/// Connection state for tracking SSE clients.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
	/// Unique connection ID.
	pub connection_id: String,
	/// Organization ID for this connection.
	pub org_id: OrgId,
	/// Environment ID this connection is subscribed to.
	pub environment_id: EnvironmentId,
	/// When the connection was established.
	pub connected_at: DateTime<Utc>,
	/// Last activity timestamp.
	pub last_activity: DateTime<Utc>,
}

impl ConnectionInfo {
	/// Create new connection info.
	pub fn new(connection_id: String, org_id: OrgId, environment_id: EnvironmentId) -> Self {
		let now = Utc::now();
		ConnectionInfo {
			connection_id,
			org_id,
			environment_id,
			connected_at: now,
			last_activity: now,
		}
	}

	/// Update the last activity timestamp.
	pub fn touch(&mut self) {
		self.last_activity = Utc::now();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_event_type() {
		assert_eq!(FlagStreamEvent::init(vec![], vec![]).event_type(), "init");
		assert_eq!(
			FlagStreamEvent::flag_updated(
				"test".to_string(),
				"prod".to_string(),
				true,
				"on".to_string(),
				VariantValue::Boolean(true)
			)
			.event_type(),
			"flag.updated"
		);
		assert_eq!(
			FlagStreamEvent::flag_archived("test".to_string()).event_type(),
			"flag.archived"
		);
		assert_eq!(
			FlagStreamEvent::flag_restored("test".to_string(), "prod".to_string(), true).event_type(),
			"flag.restored"
		);
		assert_eq!(
			FlagStreamEvent::kill_switch_activated(
				"ks".to_string(),
				vec!["flag1".to_string()],
				"emergency".to_string()
			)
			.event_type(),
			"killswitch.activated"
		);
		assert_eq!(
			FlagStreamEvent::kill_switch_deactivated("ks".to_string(), vec!["flag1".to_string()])
				.event_type(),
			"killswitch.deactivated"
		);
		assert_eq!(FlagStreamEvent::heartbeat().event_type(), "heartbeat");
	}

	#[test]
	fn test_flag_updated_serialization() {
		let event = FlagStreamEvent::flag_updated(
			"checkout.new_flow".to_string(),
			"prod".to_string(),
			true,
			"enabled".to_string(),
			VariantValue::Boolean(true),
		);

		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"flag.updated""#));
		assert!(json.contains(r#""flag_key":"checkout.new_flow""#));
		assert!(json.contains(r#""environment":"prod""#));
		assert!(json.contains(r#""enabled":true"#));
	}

	#[test]
	fn test_kill_switch_activated_serialization() {
		let event = FlagStreamEvent::kill_switch_activated(
			"disable_payments".to_string(),
			vec!["payments.enabled".to_string(), "checkout.flow".to_string()],
			"Payment provider outage".to_string(),
		);

		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"killswitch.activated""#));
		assert!(json.contains(r#""kill_switch_key":"disable_payments""#));
		assert!(json.contains(r#""reason":"Payment provider outage""#));
	}

	#[test]
	fn test_heartbeat_serialization() {
		let event = FlagStreamEvent::heartbeat();
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"heartbeat""#));
		assert!(json.contains(r#""timestamp""#));
	}

	#[test]
	fn test_init_data_serialization() {
		let flags = vec![FlagState {
			key: "feature.test".to_string(),
			id: FlagId::new(),
			enabled: true,
			default_variant: "on".to_string(),
			default_value: VariantValue::Boolean(true),
			archived: false,
		}];
		let kill_switches = vec![];

		let event = FlagStreamEvent::init(flags, kill_switches);
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"init""#));
		assert!(json.contains(r#""flags""#));
		assert!(json.contains(r#""feature.test""#));
	}

	#[test]
	fn test_deserialization_roundtrip() {
		let event = FlagStreamEvent::flag_updated(
			"test.flag".to_string(),
			"staging".to_string(),
			false,
			"off".to_string(),
			VariantValue::String("disabled".to_string()),
		);

		let json = serde_json::to_string(&event).unwrap();
		let parsed: FlagStreamEvent = serde_json::from_str(&json).unwrap();

		if let FlagStreamEvent::FlagUpdated(data) = parsed {
			assert_eq!(data.flag_key, "test.flag");
			assert_eq!(data.environment, "staging");
			assert!(!data.enabled);
		} else {
			panic!("Expected FlagUpdated event");
		}
	}

	#[test]
	fn test_connection_info() {
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();
		let mut conn = ConnectionInfo::new("conn-123".to_string(), org_id, env_id);

		assert_eq!(conn.connection_id, "conn-123");
		assert_eq!(conn.org_id, org_id);
		assert_eq!(conn.environment_id, env_id);

		let old_activity = conn.last_activity;
		std::thread::sleep(std::time::Duration::from_millis(10));
		conn.touch();
		assert!(conn.last_activity > old_activity);
	}

	#[test]
	fn test_kill_switch_state_from() {
		use crate::UserId;

		let ks = KillSwitch {
			id: KillSwitchId::new(),
			org_id: Some(OrgId::new()),
			key: "test_ks".to_string(),
			name: "Test Kill Switch".to_string(),
			description: None,
			linked_flag_keys: vec!["flag1".to_string(), "flag2".to_string()],
			is_active: true,
			activated_at: Some(Utc::now()),
			activated_by: Some(UserId::new()),
			activation_reason: Some("Testing".to_string()),
			created_at: Utc::now(),
			updated_at: Utc::now(),
		};

		let state: KillSwitchState = (&ks).into();
		assert_eq!(state.key, "test_ks");
		assert!(state.is_active);
		assert_eq!(state.linked_flag_keys.len(), 2);
		assert_eq!(state.activation_reason, Some("Testing".to_string()));
	}
}
