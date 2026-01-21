// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FdKey {
	pub pid: u32,
	pub fd: i32,
}

#[derive(Debug, Clone)]
pub struct FdState {
	pub socket_id: Option<SocketId>,
	pub created_at_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketId(pub u64);

#[derive(Debug, Clone)]
pub struct SocketState {
	pub domain: u16,
	pub socket_type: u16,
	pub protocol: u16,
	pub local_addr: Option<(IpAddr, u16)>,
	pub remote_addr: Option<(IpAddr, u16)>,
	pub hostname: Option<String>,
	pub created_at_ns: u64,
}

#[derive(Debug)]
pub struct ConnectionTracker {
	fd_table: HashMap<FdKey, FdState>,
	socket_table: HashMap<SocketId, SocketState>,
	next_socket_id: u64,
}

impl Default for ConnectionTracker {
	fn default() -> Self {
		Self::new()
	}
}

impl ConnectionTracker {
	pub fn new() -> Self {
		ConnectionTracker {
			fd_table: HashMap::new(),
			socket_table: HashMap::new(),
			next_socket_id: 1,
		}
	}

	pub fn on_socket(
		&mut self,
		pid: u32,
		fd: i32,
		domain: u16,
		socket_type: u16,
		protocol: u16,
		timestamp_ns: u64,
	) -> SocketId {
		let fd_key = FdKey { pid, fd };

		if let Some(old_state) = self.fd_table.remove(&fd_key) {
			if let Some(old_socket_id) = old_state.socket_id {
				let refcount = self
					.fd_table
					.values()
					.filter(|s| s.socket_id == Some(old_socket_id))
					.count();
				if refcount == 0 {
					self.socket_table.remove(&old_socket_id);
				}
			}
		}

		let socket_id = SocketId(self.next_socket_id);
		self.next_socket_id += 1;

		self.fd_table.insert(
			fd_key,
			FdState {
				socket_id: Some(socket_id),
				created_at_ns: timestamp_ns,
			},
		);

		self.socket_table.insert(
			socket_id,
			SocketState {
				domain,
				socket_type,
				protocol,
				local_addr: None,
				remote_addr: None,
				hostname: None,
				created_at_ns: timestamp_ns,
			},
		);

		socket_id
	}

	pub fn on_connect(
		&mut self,
		pid: u32,
		fd: i32,
		remote_addr: IpAddr,
		remote_port: u16,
		hostname: Option<String>,
	) {
		if let Some(fd_state) = self.fd_table.get(&FdKey { pid, fd }) {
			if let Some(socket_id) = fd_state.socket_id {
				if let Some(socket_state) = self.socket_table.get_mut(&socket_id) {
					socket_state.remote_addr = Some((remote_addr, remote_port));
					socket_state.hostname = hostname;
				}
			}
		}
	}

	pub fn on_close(&mut self, pid: u32, fd: i32) {
		if let Some(fd_state) = self.fd_table.remove(&FdKey { pid, fd }) {
			if let Some(socket_id) = fd_state.socket_id {
				let refcount = self
					.fd_table
					.values()
					.filter(|s| s.socket_id == Some(socket_id))
					.count();
				if refcount == 0 {
					self.socket_table.remove(&socket_id);
				}
			}
		}
	}

	pub fn on_dup(&mut self, pid: u32, old_fd: i32, new_fd: i32, timestamp_ns: u64) {
		if let Some(old_state) = self.fd_table.get(&FdKey { pid, fd: old_fd }).cloned() {
			self.fd_table.insert(
				FdKey { pid, fd: new_fd },
				FdState {
					socket_id: old_state.socket_id,
					created_at_ns: timestamp_ns,
				},
			);
		}
	}

	pub fn on_fork(&mut self, parent_pid: u32, child_pid: u32, timestamp_ns: u64) {
		let parent_fds: Vec<_> = self
			.fd_table
			.iter()
			.filter(|(k, _)| k.pid == parent_pid)
			.map(|(k, v)| (k.fd, v.clone()))
			.collect();

		for (fd, state) in parent_fds {
			self.fd_table.insert(
				FdKey { pid: child_pid, fd },
				FdState {
					socket_id: state.socket_id,
					created_at_ns: timestamp_ns,
				},
			);
		}
	}

	pub fn on_exit(&mut self, pid: u32) {
		let fds_to_remove: Vec<_> = self
			.fd_table
			.keys()
			.filter(|k| k.pid == pid)
			.cloned()
			.collect();

		for key in fds_to_remove {
			self.on_close(key.pid, key.fd);
		}
	}

	pub fn get_socket(&self, pid: u32, fd: i32) -> Option<&SocketState> {
		self
			.fd_table
			.get(&FdKey { pid, fd })
			.and_then(|fd_state| fd_state.socket_id)
			.and_then(|socket_id| self.socket_table.get(&socket_id))
	}

	pub fn fd_count(&self) -> usize {
		self.fd_table.len()
	}

	pub fn socket_count(&self) -> usize {
		self.socket_table.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::net::Ipv4Addr;

	#[test]
	fn test_socket_lifecycle() {
		let mut tracker = ConnectionTracker::new();

		let socket_id = tracker.on_socket(100, 5, 2, 1, 6, 1000);
		assert!(tracker.get_socket(100, 5).is_some());

		tracker.on_connect(
			100,
			5,
			IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
			443,
			Some("dns.google".to_string()),
		);
		let socket = tracker.get_socket(100, 5).unwrap();
		assert_eq!(
			socket.remote_addr,
			Some((IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 443))
		);
		assert_eq!(socket.hostname, Some("dns.google".to_string()));

		tracker.on_close(100, 5);
		assert!(tracker.get_socket(100, 5).is_none());
		assert_eq!(tracker.socket_count(), 0);
	}

	#[test]
	fn test_dup_shares_socket() {
		let mut tracker = ConnectionTracker::new();

		tracker.on_socket(100, 5, 2, 1, 6, 1000);
		tracker.on_dup(100, 5, 10, 2000);

		assert!(tracker.get_socket(100, 5).is_some());
		assert!(tracker.get_socket(100, 10).is_some());

		tracker.on_close(100, 5);
		assert!(tracker.get_socket(100, 10).is_some());
		assert_eq!(tracker.socket_count(), 1);

		tracker.on_close(100, 10);
		assert_eq!(tracker.socket_count(), 0);
	}

	#[test]
	fn test_fork_copies_fds() {
		let mut tracker = ConnectionTracker::new();

		tracker.on_socket(100, 5, 2, 1, 6, 1000);
		tracker.on_fork(100, 200, 2000);

		assert!(tracker.get_socket(100, 5).is_some());
		assert!(tracker.get_socket(200, 5).is_some());
	}

	#[test]
	fn test_exit_cleans_up() {
		let mut tracker = ConnectionTracker::new();

		tracker.on_socket(100, 5, 2, 1, 6, 1000);
		tracker.on_socket(100, 6, 2, 1, 6, 1000);
		tracker.on_socket(200, 5, 2, 1, 6, 1000);

		tracker.on_exit(100);

		assert!(tracker.get_socket(100, 5).is_none());
		assert!(tracker.get_socket(100, 6).is_none());
		assert!(tracker.get_socket(200, 5).is_some());
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	#[derive(Debug, Clone)]
	enum TrackerOp {
		Socket { pid: u32, fd: i32 },
		Connect { pid: u32, fd: i32 },
		Close { pid: u32, fd: i32 },
		Dup { pid: u32, old_fd: i32, new_fd: i32 },
		Fork { parent: u32, child: u32 },
		Exit { pid: u32 },
	}

	fn tracker_op_strategy() -> impl Strategy<Value = TrackerOp> {
		prop_oneof![
			(1..10u32, 0..20i32).prop_map(|(pid, fd)| TrackerOp::Socket { pid, fd }),
			(1..10u32, 0..20i32).prop_map(|(pid, fd)| TrackerOp::Connect { pid, fd }),
			(1..10u32, 0..20i32).prop_map(|(pid, fd)| TrackerOp::Close { pid, fd }),
			(1..10u32, 0..20i32, 0..20i32).prop_map(|(pid, old_fd, new_fd)| TrackerOp::Dup {
				pid,
				old_fd,
				new_fd
			}),
			(1..10u32, 10..20u32).prop_map(|(parent, child)| TrackerOp::Fork { parent, child }),
			(1..20u32).prop_map(|pid| TrackerOp::Exit { pid }),
		]
	}

	proptest! {
		#[test]
		fn test_tracker_never_panics(ops in prop::collection::vec(tracker_op_strategy(), 0..100)) {
			let mut tracker = ConnectionTracker::new();
			let mut ts = 0u64;

			for op in ops {
				ts += 1;
				match op {
					TrackerOp::Socket { pid, fd } => {
						tracker.on_socket(pid, fd, 2, 1, 6, ts);
					}
					TrackerOp::Connect { pid, fd } => {
						tracker.on_connect(pid, fd, IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 80, None);
					}
					TrackerOp::Close { pid, fd } => {
						tracker.on_close(pid, fd);
					}
					TrackerOp::Dup { pid, old_fd, new_fd } => {
						tracker.on_dup(pid, old_fd, new_fd, ts);
					}
					TrackerOp::Fork { parent, child } => {
						tracker.on_fork(parent, child, ts);
					}
					TrackerOp::Exit { pid } => {
						tracker.on_exit(pid);
					}
				}
			}

			prop_assert!(tracker.socket_count() <= tracker.fd_count() + 1);
		}
	}
}
