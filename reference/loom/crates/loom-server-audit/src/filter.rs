// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};

use crate::enrichment::EnrichedAuditEvent;
use crate::event::{AuditEventType, AuditSeverity};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFilterConfig {
	pub min_severity: AuditSeverity,
	pub include_events: Option<Vec<AuditEventType>>,
	pub exclude_events: Option<Vec<AuditEventType>>,
}

impl Default for AuditFilterConfig {
	fn default() -> Self {
		Self {
			min_severity: AuditSeverity::Info,
			include_events: None,
			exclude_events: None,
		}
	}
}

impl AuditFilterConfig {
	pub fn allows(&self, event: &EnrichedAuditEvent) -> bool {
		if event.base.severity < self.min_severity {
			return false;
		}

		if let Some(ref exclude) = self.exclude_events {
			if exclude.contains(&event.base.event_type) {
				return false;
			}
		}

		if let Some(ref include) = self.include_events {
			if !include.contains(&event.base.event_type) {
				return false;
			}
		}

		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::event::AuditLogEntry;

	fn make_event(event_type: AuditEventType, severity: AuditSeverity) -> EnrichedAuditEvent {
		EnrichedAuditEvent {
			base: AuditLogEntry::builder(event_type)
				.severity(severity)
				.build(),
			session: None,
			org: None,
		}
	}

	#[test]
	fn test_default_config_allows_info_and_above() {
		let config = AuditFilterConfig::default();

		let info_event = make_event(AuditEventType::Login, AuditSeverity::Info);
		assert!(config.allows(&info_event));

		let warning_event = make_event(AuditEventType::LoginFailed, AuditSeverity::Warning);
		assert!(config.allows(&warning_event));

		let debug_event = make_event(AuditEventType::Login, AuditSeverity::Debug);
		assert!(!config.allows(&debug_event));
	}

	#[test]
	fn test_min_severity_filter() {
		let config = AuditFilterConfig {
			min_severity: AuditSeverity::Warning,
			include_events: None,
			exclude_events: None,
		};

		let info_event = make_event(AuditEventType::Login, AuditSeverity::Info);
		assert!(!config.allows(&info_event));

		let warning_event = make_event(AuditEventType::LoginFailed, AuditSeverity::Warning);
		assert!(config.allows(&warning_event));

		let error_event = make_event(AuditEventType::AccessDenied, AuditSeverity::Error);
		assert!(config.allows(&error_event));
	}

	#[test]
	fn test_include_events_whitelist() {
		let config = AuditFilterConfig {
			min_severity: AuditSeverity::Info,
			include_events: Some(vec![AuditEventType::Login, AuditEventType::Logout]),
			exclude_events: None,
		};

		let login_event = make_event(AuditEventType::Login, AuditSeverity::Info);
		assert!(config.allows(&login_event));

		let logout_event = make_event(AuditEventType::Logout, AuditSeverity::Info);
		assert!(config.allows(&logout_event));

		let api_key_event = make_event(AuditEventType::ApiKeyCreated, AuditSeverity::Info);
		assert!(!config.allows(&api_key_event));
	}

	#[test]
	fn test_exclude_events_blacklist() {
		let config = AuditFilterConfig {
			min_severity: AuditSeverity::Info,
			include_events: None,
			exclude_events: Some(vec![AuditEventType::ApiKeyUsed]),
		};

		let login_event = make_event(AuditEventType::Login, AuditSeverity::Info);
		assert!(config.allows(&login_event));

		let api_event = make_event(AuditEventType::ApiKeyUsed, AuditSeverity::Info);
		assert!(!config.allows(&api_event));
	}

	#[test]
	fn test_exclude_takes_precedence_over_include() {
		let config = AuditFilterConfig {
			min_severity: AuditSeverity::Info,
			include_events: Some(vec![AuditEventType::Login, AuditEventType::Logout]),
			exclude_events: Some(vec![AuditEventType::Login]),
		};

		let login_event = make_event(AuditEventType::Login, AuditSeverity::Info);
		assert!(!config.allows(&login_event));

		let logout_event = make_event(AuditEventType::Logout, AuditSeverity::Info);
		assert!(config.allows(&logout_event));
	}

	#[test]
	fn test_severity_checked_before_event_type() {
		let config = AuditFilterConfig {
			min_severity: AuditSeverity::Warning,
			include_events: Some(vec![AuditEventType::Login]),
			exclude_events: None,
		};

		let info_login = make_event(AuditEventType::Login, AuditSeverity::Info);
		assert!(!config.allows(&info_login));

		let warning_login = make_event(AuditEventType::Login, AuditSeverity::Warning);
		assert!(config.allows(&warning_login));
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use crate::event::AuditLogEntry;
	use proptest::prelude::*;

	fn arb_severity() -> impl Strategy<Value = AuditSeverity> {
		prop_oneof![
			Just(AuditSeverity::Debug),
			Just(AuditSeverity::Info),
			Just(AuditSeverity::Notice),
			Just(AuditSeverity::Warning),
			Just(AuditSeverity::Error),
			Just(AuditSeverity::Critical),
		]
	}

	fn arb_event_type() -> impl Strategy<Value = AuditEventType> {
		prop_oneof![
			Just(AuditEventType::Login),
			Just(AuditEventType::Logout),
			Just(AuditEventType::LoginFailed),
			Just(AuditEventType::SessionCreated),
			Just(AuditEventType::SessionRevoked),
			Just(AuditEventType::ApiKeyCreated),
			Just(AuditEventType::ApiKeyUsed),
			Just(AuditEventType::AccessGranted),
			Just(AuditEventType::AccessDenied),
		]
	}

	fn make_event(event_type: AuditEventType, severity: AuditSeverity) -> EnrichedAuditEvent {
		EnrichedAuditEvent {
			base: AuditLogEntry::builder(event_type)
				.severity(severity)
				.build(),
			session: None,
			org: None,
		}
	}

	proptest! {
		#[test]
		fn prop_severity_filter_is_monotonic(
			min_sev in arb_severity(),
			event_sev in arb_severity(),
			event_type in arb_event_type()
		) {
			let config = AuditFilterConfig {
				min_severity: min_sev,
				include_events: None,
				exclude_events: None,
			};
			let event = make_event(event_type, event_sev);
			let allowed = config.allows(&event);

			if event_sev >= min_sev {
				prop_assert!(allowed, "Event with severity {:?} should pass min {:?}", event_sev, min_sev);
			} else {
				prop_assert!(!allowed, "Event with severity {:?} should NOT pass min {:?}", event_sev, min_sev);
			}
		}

		#[test]
		fn prop_excluded_events_never_pass(
			event_type in arb_event_type(),
			severity in arb_severity()
		) {
			let config = AuditFilterConfig {
				min_severity: AuditSeverity::Debug,
				include_events: None,
				exclude_events: Some(vec![event_type]),
			};
			let event = make_event(event_type, severity);
			prop_assert!(!config.allows(&event), "Excluded event type should never pass");
		}

		#[test]
		fn prop_include_whitelist_only_allows_listed(
			target_type in arb_event_type(),
			test_type in arb_event_type(),
			severity in arb_severity()
		) {
			let config = AuditFilterConfig {
				min_severity: AuditSeverity::Debug,
				include_events: Some(vec![target_type]),
				exclude_events: None,
			};
			let event = make_event(test_type, severity);
			let allowed = config.allows(&event);

			if test_type == target_type {
				prop_assert!(allowed, "Included event type should pass");
			} else {
				prop_assert!(!allowed, "Non-included event type should not pass");
			}
		}

		#[test]
		fn prop_default_config_allows_info_and_above(
			event_type in arb_event_type(),
			severity in arb_severity()
		) {
			let config = AuditFilterConfig::default();
			let event = make_event(event_type, severity);
			let allowed = config.allows(&event);

			if severity >= AuditSeverity::Info {
				prop_assert!(allowed);
			} else {
				prop_assert!(!allowed);
			}
		}
	}
}
