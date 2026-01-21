// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

const SENSITIVE_PATH_PREFIXES: &[&str] = &[
	"/etc/passwd",
	"/etc/shadow",
	"/etc/sudoers",
	"/etc/ssh",
	"/root",
	"/home",
	"/.ssh",
	"/.gnupg",
	"/.aws",
	"/.config",
	"/proc/",
	"/sys/",
];

const IGNORE_PATH_PREFIXES: &[&str] = &[
	"/proc/self/fd",
	"/dev/null",
	"/dev/zero",
	"/dev/urandom",
	"/dev/random",
	"/usr/share/zoneinfo",
	"/usr/lib/locale",
];

#[derive(Debug, Clone)]
pub struct PathFilter {
	always_capture_prefixes: Vec<String>,
	ignore_prefixes: Vec<String>,
	sensitive_prefixes: Vec<String>,
}

impl Default for PathFilter {
	fn default() -> Self {
		Self::new()
	}
}

impl PathFilter {
	pub fn new() -> Self {
		PathFilter {
			always_capture_prefixes: Vec::new(),
			ignore_prefixes: IGNORE_PATH_PREFIXES.iter().map(|s| s.to_string()).collect(),
			sensitive_prefixes: SENSITIVE_PATH_PREFIXES
				.iter()
				.map(|s| s.to_string())
				.collect(),
		}
	}

	#[allow(dead_code)] // Runtime configuration API
	pub fn add_always_capture(&mut self, prefix: String) {
		self.always_capture_prefixes.push(prefix);
	}

	#[allow(dead_code)] // Runtime configuration API
	pub fn add_ignore(&mut self, prefix: String) {
		self.ignore_prefixes.push(prefix);
	}

	pub fn should_capture_file_event(&self, path: &str, is_write: bool) -> bool {
		for prefix in &self.ignore_prefixes {
			if path.starts_with(prefix) {
				return false;
			}
		}

		if is_write {
			return true;
		}

		for prefix in &self.always_capture_prefixes {
			if path.starts_with(prefix) {
				return true;
			}
		}

		for prefix in &self.sensitive_prefixes {
			if path.starts_with(prefix) {
				return true;
			}
		}

		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_writes_always_captured() {
		let filter = PathFilter::new();
		assert!(filter.should_capture_file_event("/some/random/file.txt", true));
		assert!(filter.should_capture_file_event("/usr/lib/whatever.so", true));
	}

	#[test]
	fn test_ignored_paths() {
		let filter = PathFilter::new();
		assert!(!filter.should_capture_file_event("/dev/null", false));
		assert!(!filter.should_capture_file_event("/dev/null", true));
		assert!(!filter.should_capture_file_event("/proc/self/fd/5", false));
	}

	#[test]
	fn test_sensitive_reads_captured() {
		let filter = PathFilter::new();
		assert!(filter.should_capture_file_event("/etc/passwd", false));
		assert!(filter.should_capture_file_event("/etc/shadow", false));
		assert!(filter.should_capture_file_event("/home/user/.bashrc", false));
		assert!(filter.should_capture_file_event("/root/.ssh/id_rsa", false));
	}

	#[test]
	fn test_non_sensitive_reads_ignored() {
		let filter = PathFilter::new();
		assert!(!filter.should_capture_file_event("/usr/lib/libfoo.so", false));
		assert!(!filter.should_capture_file_event("/opt/app/data.txt", false));
	}

	#[test]
	fn test_custom_always_capture() {
		let mut filter = PathFilter::new();
		filter.add_always_capture("/workspace".to_string());
		assert!(filter.should_capture_file_event("/workspace/project/main.rs", false));
	}
}
