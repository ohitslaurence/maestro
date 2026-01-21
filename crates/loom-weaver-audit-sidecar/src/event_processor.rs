// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ptr;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

use crate::metrics::Metrics;

use loom_weaver_ebpf_common::{
	DnsQueryEvent, DnsResponseEvent, EventHeader, EventType, FileEvent, FileOpenEvent,
	MemoryExecEvent, NetworkAcceptEvent, NetworkConnectEvent, NetworkListenEvent, NetworkSocketEvent,
	PrivilegeChangeEvent, ProcessExecEvent, ProcessExitEvent, ProcessForkEvent, SandboxEscapeEvent,
	MAX_COMM_LEN,
};

use crate::config::Config;
use crate::connection_tracker::ConnectionTracker;
use crate::dns_cache::DnsCache;
use crate::events::{
	bytes_to_string, DnsQueryDetails, DnsResponseDetails, FileEventDetails, MemoryExecDetails,
	NetworkConnectDetails, NetworkSocketDetails, PrivilegeChangeDetails, ProcessExecDetails,
	ProcessExitDetails, ProcessForkDetails, SandboxEscapeDetails, WeaverAuditEvent,
	WeaverAuditEventType,
};
use crate::filter::PathFilter;

fn read_event<T: Copy>(data: &[u8]) -> Option<T> {
	if data.len() < std::mem::size_of::<T>() {
		return None;
	}
	let ptr = data.as_ptr() as *const T;
	Some(unsafe { ptr::read_unaligned(ptr) })
}

pub struct EventProcessorConfig {
	pub weaver_id: String,
	pub org_id: String,
	pub owner_user_id: String,
}

impl From<&Config> for EventProcessorConfig {
	fn from(config: &Config) -> Self {
		Self {
			weaver_id: config.weaver_id.clone(),
			org_id: config.org_id.clone(),
			owner_user_id: config.owner_user_id.clone(),
		}
	}
}

pub struct EventProcessor {
	config: EventProcessorConfig,
	tx: mpsc::Sender<WeaverAuditEvent>,
	dns_cache: Mutex<DnsCache>,
	connection_tracker: Mutex<ConnectionTracker>,
	filter: PathFilter,
	metrics: Arc<Metrics>,
}

impl EventProcessor {
	pub fn new(
		config: EventProcessorConfig,
		tx: mpsc::Sender<WeaverAuditEvent>,
		metrics: Arc<Metrics>,
	) -> Self {
		Self {
			config,
			tx,
			dns_cache: Mutex::new(DnsCache::new()),
			connection_tracker: Mutex::new(ConnectionTracker::new()),
			filter: PathFilter::new(),
			metrics,
		}
	}

	pub async fn process_raw_event(&self, data: &[u8]) -> bool {
		let header: EventHeader = match read_event(data) {
			Some(h) => h,
			None => {
				warn!(len = data.len(), "Event too small for header");
				return false;
			}
		};

		let event_type = match EventType::from_u32(header.event_type) {
			Some(et) => et,
			None => {
				warn!(event_type = header.event_type, "Unknown event type");
				return false;
			}
		};

		trace!(?event_type, pid = header.pid, "Processing eBPF event");

		let result = match event_type {
			EventType::ProcessExec => self.process_exec(data).await,
			EventType::ProcessFork => self.process_fork(data).await,
			EventType::ProcessExit => self.process_exit(data).await,
			EventType::FileWrite
			| EventType::FileRead
			| EventType::FileMetadata
			| EventType::FileOpen => self.process_file(data, event_type).await,
			EventType::NetworkSocket => self.process_socket(data).await,
			EventType::NetworkConnect => self.process_connect(data).await,
			EventType::NetworkListen => self.process_listen(data).await,
			EventType::NetworkAccept => self.process_accept(data).await,
			EventType::DnsQuery => self.process_dns_query(data).await,
			EventType::DnsResponse => self.process_dns_response(data).await,
			EventType::PrivilegeChange => self.process_privilege_change(data).await,
			EventType::MemoryExec => self.process_memory_exec(data).await,
			EventType::SandboxEscape => self.process_sandbox_escape(data).await,
		};

		if let Err(e) = result {
			warn!(?event_type, error = %e, "Failed to process event");
			return false;
		}

		let audit_event_type = match event_type {
			EventType::ProcessExec => WeaverAuditEventType::ProcessExec,
			EventType::ProcessFork => WeaverAuditEventType::ProcessFork,
			EventType::ProcessExit => WeaverAuditEventType::ProcessExit,
			EventType::FileWrite => WeaverAuditEventType::FileWrite,
			EventType::FileRead => WeaverAuditEventType::FileRead,
			EventType::FileMetadata => WeaverAuditEventType::FileMetadata,
			EventType::FileOpen => WeaverAuditEventType::FileOpen,
			EventType::NetworkSocket => WeaverAuditEventType::NetworkSocket,
			EventType::NetworkConnect => WeaverAuditEventType::NetworkConnect,
			EventType::NetworkListen => WeaverAuditEventType::NetworkListen,
			EventType::NetworkAccept => WeaverAuditEventType::NetworkAccept,
			EventType::DnsQuery => WeaverAuditEventType::DnsQuery,
			EventType::DnsResponse => WeaverAuditEventType::DnsResponse,
			EventType::PrivilegeChange => WeaverAuditEventType::PrivilegeChange,
			EventType::MemoryExec => WeaverAuditEventType::MemoryExec,
			EventType::SandboxEscape => WeaverAuditEventType::SandboxEscape,
		};
		self.metrics.record_event_captured(audit_event_type);

		true
	}

	async fn send_event(
		&self,
		header: &EventHeader,
		comm: &[u8; MAX_COMM_LEN],
		event_type: WeaverAuditEventType,
		details: serde_json::Value,
	) -> anyhow::Result<()> {
		let event = WeaverAuditEvent {
			weaver_id: self.config.weaver_id.clone(),
			org_id: self.config.org_id.clone(),
			owner_user_id: self.config.owner_user_id.clone(),
			timestamp_ns: header.timestamp_ns,
			pid: header.pid,
			tid: header.tid,
			comm: bytes_to_string(comm),
			event_type,
			details,
		};

		self
			.tx
			.send(event)
			.await
			.map_err(|e| anyhow::anyhow!("channel send failed: {}", e))
	}

	async fn process_exec(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: ProcessExecEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for ProcessExecEvent"))?;
		let path = bytes_to_string(&event.filename);

		let details = ProcessExecDetails {
			path,
			argv: vec![],
			cwd: None,
			uid: event.header.uid,
			gid: event.header.gid,
			ppid: event.header.pid,
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::ProcessExec,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_fork(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: ProcessForkEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for ProcessForkEvent"))?;
		let details = ProcessForkDetails {
			parent_pid: event.parent_pid,
			child_pid: event.child_pid,
			clone_flags: event.clone_flags,
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::ProcessFork,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_exit(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: ProcessExitEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for ProcessExitEvent"))?;

		if let Ok(mut tracker) = self.connection_tracker.lock() {
			tracker.on_exit(event.header.pid);
		}

		let details = ProcessExitDetails {
			exit_code: event.exit_code,
		};

		self
			.send_event(
				&event.header,
				&event.comm,
				WeaverAuditEventType::ProcessExit,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_file(&self, data: &[u8], event_type: EventType) -> anyhow::Result<()> {
		// FileOpen uses FileOpenEvent (304 bytes), other file events use FileEvent (320 bytes)
		let (header, path, flags, mode) = if event_type == EventType::FileOpen {
			let event: FileOpenEvent =
				read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for FileOpenEvent"))?;
			(
				event.header,
				bytes_to_string(&event.filename),
				event.flags as u32,
				0u32,
			)
		} else {
			let event: FileEvent =
				read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for FileEvent"))?;
			(
				event.header,
				bytes_to_string(&event.path),
				event.flags,
				event.mode,
			)
		};

		let is_write = matches!(event_type, EventType::FileWrite | EventType::FileMetadata);
		if !self.filter.should_capture_file_event(&path, is_write) {
			return Ok(());
		}

		let details = FileEventDetails { path, flags, mode };

		let audit_type = match event_type {
			EventType::FileWrite => WeaverAuditEventType::FileWrite,
			EventType::FileRead => WeaverAuditEventType::FileRead,
			EventType::FileMetadata => WeaverAuditEventType::FileMetadata,
			EventType::FileOpen => WeaverAuditEventType::FileOpen,
			_ => unreachable!(),
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(&header, &comm, audit_type, serde_json::to_value(details)?)
			.await
	}

	async fn process_socket(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: NetworkSocketEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for NetworkSocketEvent"))?;

		if let Ok(mut tracker) = self.connection_tracker.lock() {
			tracker.on_socket(
				event.header.pid,
				event.fd,
				event.domain as u16,
				event.sock_type as u16,
				event.protocol as u16,
				event.header.timestamp_ns,
			);
		}

		let details = NetworkSocketDetails {
			domain: event.domain as u16,
			socket_type: event.sock_type as u16,
			protocol: event.protocol as u16,
			fd: event.fd,
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::NetworkSocket,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_connect(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: NetworkConnectEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for NetworkConnectEvent"))?;
		let (remote_ip_str, remote_ip) = format_ip_address(event.family, &event.addr, event.addr_len);
		let hostname = remote_ip.and_then(|ip| self.dns_cache.lock().ok()?.lookup(&ip));

		if let Some(ip) = remote_ip {
			if let Ok(mut tracker) = self.connection_tracker.lock() {
				tracker.on_connect(event.header.pid, event.fd, ip, event.port, hostname.clone());
			}
		}

		let details = NetworkConnectDetails {
			fd: event.fd,
			remote_ip: remote_ip_str,
			remote_port: event.port,
			hostname,
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::NetworkConnect,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_listen(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: NetworkListenEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for NetworkListenEvent"))?;
		let details = serde_json::json!({
			"fd": event.fd,
			"backlog": event.backlog,
			"port": event.port,
		});

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::NetworkListen,
				details,
			)
			.await
	}

	async fn process_accept(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: NetworkAcceptEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for NetworkAcceptEvent"))?;
		let (remote_ip, _) = format_ip_address(event.family, &event.addr, event.addr_len);

		let details = serde_json::json!({
			"listen_fd": event.listen_fd,
			"conn_fd": event.conn_fd,
			"remote_ip": remote_ip,
			"remote_port": event.port,
		});

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::NetworkAccept,
				details,
			)
			.await
	}

	async fn process_dns_query(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: DnsQueryEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for DnsQueryEvent"))?;
		let details = DnsQueryDetails {
			query: bytes_to_string(&event.hostname),
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::DnsQuery,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_dns_response(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: DnsResponseEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for DnsResponseEvent"))?;
		let query = bytes_to_string(&event.hostname);
		let (addr_str, addr_ip) = format_ip_address(2, &event.addr, event.addr_len);

		if let Some(ip) = addr_ip {
			if let Ok(mut cache) = self.dns_cache.lock() {
				cache.insert(ip, query.clone(), event.ttl);
			}
		}

		let details = DnsResponseDetails {
			query,
			addresses: vec![addr_str],
			ttl: event.ttl,
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::DnsResponse,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_privilege_change(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: PrivilegeChangeEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for PrivilegeChangeEvent"))?;
		let syscall = match event.change_type {
			1 => "setuid",
			2 => "setgid",
			3 => "setreuid",
			4 => "setregid",
			5 => "setresuid",
			6 => "setresgid",
			_ => "unknown",
		};

		let details = PrivilegeChangeDetails {
			syscall: syscall.to_string(),
			old_uid: Some(event.old_uid),
			new_uid: Some(event.new_uid),
			old_gid: Some(event.old_gid),
			new_gid: Some(event.new_gid),
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::PrivilegeChange,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_memory_exec(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: MemoryExecEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for MemoryExecEvent"))?;
		let details = MemoryExecDetails {
			addr: event.addr,
			len: event.len,
			prot: event.prot,
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::MemoryExec,
				serde_json::to_value(details)?,
			)
			.await
	}

	async fn process_sandbox_escape(&self, data: &[u8]) -> anyhow::Result<()> {
		let event: SandboxEscapeEvent =
			read_event(data).ok_or_else(|| anyhow::anyhow!("data too small for SandboxEscapeEvent"))?;
		let syscall = format!("syscall_{}", event.syscall_nr);

		let details = SandboxEscapeDetails {
			syscall,
			arg0: event.arg0,
			arg1: event.arg1,
		};

		let comm = [0u8; MAX_COMM_LEN];
		self
			.send_event(
				&event.header,
				&comm,
				WeaverAuditEventType::SandboxEscape,
				serde_json::to_value(details)?,
			)
			.await
	}
}

fn format_ip_address(family: u16, addr: &[u8], addr_len: u8) -> (String, Option<IpAddr>) {
	let len = (addr_len as usize).min(addr.len());

	match family {
		2 if len >= 4 => {
			let ip = Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
			(ip.to_string(), Some(IpAddr::V4(ip)))
		}
		10 if len >= 16 => {
			let mut octets = [0u8; 16];
			octets.copy_from_slice(&addr[..16]);
			let ip = Ipv6Addr::from(octets);
			(ip.to_string(), Some(IpAddr::V6(ip)))
		}
		_ => (format!("unknown:{:?}", &addr[..len]), None),
	}
}

#[allow(dead_code)] // Convenience constructor for external use
pub fn spawn_event_processor(
	config: EventProcessorConfig,
	tx: mpsc::Sender<WeaverAuditEvent>,
	metrics: Arc<Metrics>,
) -> EventProcessor {
	debug!("Created event processor for weaver {}", config.weaver_id);
	EventProcessor::new(config, tx, metrics)
}
