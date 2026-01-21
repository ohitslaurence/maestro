// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Fingerprinting algorithm for grouping similar crash events into issues.

use sha2::{Digest, Sha256};

use crate::event::CrashEvent;

/// Compute a fingerprint for a crash event to group similar crashes.
///
/// The fingerprint is a SHA256 hash based on:
/// 1. Exception type (most significant)
/// 2. Top N in-app frames (function + module)
/// 3. If no in-app frames, use all frames
pub fn compute_fingerprint(event: &CrashEvent) -> String {
	let mut hasher = Sha256::new();

	// 1. Exception type (most significant)
	hasher.update(event.exception_type.as_bytes());
	hasher.update(b"|");

	// 2. Top N in-app frames (function + module)
	let in_app_frames: Vec<_> = event
		.stacktrace
		.frames
		.iter()
		.filter(|f| f.in_app)
		.take(5)
		.collect();

	for frame in &in_app_frames {
		if let Some(func) = &frame.function {
			hasher.update(func.as_bytes());
		}
		hasher.update(b"@");
		if let Some(module) = &frame.module {
			hasher.update(module.as_bytes());
		}
		hasher.update(b"|");
	}

	// 3. If no in-app frames, use all frames
	if in_app_frames.is_empty() {
		for frame in event.stacktrace.frames.iter().take(5) {
			if let Some(func) = &frame.function {
				hasher.update(func.as_bytes());
			}
			hasher.update(b"|");
		}
	}

	hex::encode(hasher.finalize())
}

/// Find the culprit function (top in-app frame).
pub fn find_culprit(event: &CrashEvent) -> Option<String> {
	event
		.stacktrace
		.frames
		.iter()
		.find(|f| f.in_app)
		.and_then(|f| f.function.clone())
}

/// Truncate a string to a maximum length with ellipsis.
pub fn truncate(s: &str, max_len: usize) -> String {
	if s.len() <= max_len {
		s.to_string()
	} else {
		format!("{}...", &s[..max_len.saturating_sub(3)])
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::event::{Frame, Stacktrace};

	#[test]
	fn fingerprint_with_in_app_frames() {
		let event = CrashEvent {
			exception_type: "TypeError".to_string(),
			stacktrace: Stacktrace {
				frames: vec![
					Frame {
						function: Some("handleClick".to_string()),
						module: Some("src/components/Button".to_string()),
						in_app: true,
						..Default::default()
					},
					Frame {
						function: Some("dispatchEvent".to_string()),
						module: Some("react-dom".to_string()),
						in_app: false,
						..Default::default()
					},
				],
			},
			..Default::default()
		};

		let fingerprint = compute_fingerprint(&event);

		// Fingerprint should be a valid hex-encoded SHA256
		assert_eq!(fingerprint.len(), 64);
		assert!(fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
	}

	#[test]
	fn fingerprint_without_in_app_frames() {
		let event = CrashEvent {
			exception_type: "Error".to_string(),
			stacktrace: Stacktrace {
				frames: vec![
					Frame {
						function: Some("someFunction".to_string()),
						in_app: false,
						..Default::default()
					},
					Frame {
						function: Some("anotherFunction".to_string()),
						in_app: false,
						..Default::default()
					},
				],
			},
			..Default::default()
		};

		let fingerprint = compute_fingerprint(&event);

		assert_eq!(fingerprint.len(), 64);
	}

	#[test]
	fn same_crash_same_fingerprint() {
		let event1 = CrashEvent {
			exception_type: "TypeError".to_string(),
			exception_value: "Cannot read property 'x' of undefined".to_string(),
			stacktrace: Stacktrace {
				frames: vec![Frame {
					function: Some("handleClick".to_string()),
					module: Some("src/components/Button".to_string()),
					in_app: true,
					..Default::default()
				}],
			},
			..Default::default()
		};

		let event2 = CrashEvent {
			exception_type: "TypeError".to_string(),
			exception_value: "Cannot read property 'y' of undefined".to_string(), // Different value
			stacktrace: Stacktrace {
				frames: vec![Frame {
					function: Some("handleClick".to_string()),
					module: Some("src/components/Button".to_string()),
					in_app: true,
					..Default::default()
				}],
			},
			..Default::default()
		};

		// Same type + same frames = same fingerprint
		assert_eq!(compute_fingerprint(&event1), compute_fingerprint(&event2));
	}

	#[test]
	fn different_type_different_fingerprint() {
		let event1 = CrashEvent {
			exception_type: "TypeError".to_string(),
			stacktrace: Stacktrace {
				frames: vec![Frame {
					function: Some("handleClick".to_string()),
					in_app: true,
					..Default::default()
				}],
			},
			..Default::default()
		};

		let event2 = CrashEvent {
			exception_type: "ReferenceError".to_string(),
			stacktrace: Stacktrace {
				frames: vec![Frame {
					function: Some("handleClick".to_string()),
					in_app: true,
					..Default::default()
				}],
			},
			..Default::default()
		};

		assert_ne!(compute_fingerprint(&event1), compute_fingerprint(&event2));
	}

	#[test]
	fn find_culprit_returns_in_app_function() {
		let event = CrashEvent {
			stacktrace: Stacktrace {
				frames: vec![
					Frame {
						function: Some("external".to_string()),
						in_app: false,
						..Default::default()
					},
					Frame {
						function: Some("myFunction".to_string()),
						in_app: true,
						..Default::default()
					},
				],
			},
			..Default::default()
		};

		assert_eq!(find_culprit(&event), Some("myFunction".to_string()));
	}

	#[test]
	fn truncate_short_string() {
		assert_eq!(truncate("hello", 10), "hello");
	}

	#[test]
	fn truncate_long_string() {
		assert_eq!(
			truncate("hello world this is a long string", 15),
			"hello world ..."
		);
	}
}
