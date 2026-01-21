// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaverAuditEventType {
	ProcessExec,
	ProcessFork,
	ProcessExit,
	FileWrite,
	FileRead,
	FileMetadata,
	FileOpen,
	NetworkSocket,
	NetworkConnect,
	NetworkListen,
	NetworkAccept,
	DnsQuery,
	DnsResponse,
	PrivilegeChange,
	MemoryExec,
	SandboxEscape,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaverAuditEvent {
	pub weaver_id: String,
	pub org_id: String,
	pub owner_user_id: String,
	pub timestamp_ns: u64,
	pub pid: u32,
	pub tid: u32,
	pub comm: String,
	pub event_type: WeaverAuditEventType,
	pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessExecDetails {
	pub path: String,
	pub argv: Vec<String>,
	pub cwd: Option<String>,
	pub uid: u32,
	pub gid: u32,
	pub ppid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessForkDetails {
	pub parent_pid: u32,
	pub child_pid: u32,
	pub clone_flags: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessExitDetails {
	pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEventDetails {
	pub path: String,
	pub flags: u32,
	pub mode: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSocketDetails {
	pub domain: u16,
	pub socket_type: u16,
	pub protocol: u16,
	pub fd: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConnectDetails {
	pub fd: i32,
	pub remote_ip: String,
	pub remote_port: u16,
	pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsQueryDetails {
	pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsResponseDetails {
	pub query: String,
	pub addresses: Vec<String>,
	pub ttl: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivilegeChangeDetails {
	pub syscall: String,
	pub old_uid: Option<u32>,
	pub new_uid: Option<u32>,
	pub old_gid: Option<u32>,
	pub new_gid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryExecDetails {
	pub addr: u64,
	pub len: u64,
	pub prot: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEscapeDetails {
	pub syscall: String,
	pub arg0: u64,
	pub arg1: u64,
}

pub fn bytes_to_string(bytes: &[u8]) -> String {
	let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
	String::from_utf8_lossy(&bytes[..end]).to_string()
}

impl From<loom_weaver_ebpf_common::EventType> for WeaverAuditEventType {
	fn from(et: loom_weaver_ebpf_common::EventType) -> Self {
		match et {
			loom_weaver_ebpf_common::EventType::ProcessExec => WeaverAuditEventType::ProcessExec,
			loom_weaver_ebpf_common::EventType::ProcessFork => WeaverAuditEventType::ProcessFork,
			loom_weaver_ebpf_common::EventType::ProcessExit => WeaverAuditEventType::ProcessExit,
			loom_weaver_ebpf_common::EventType::FileWrite => WeaverAuditEventType::FileWrite,
			loom_weaver_ebpf_common::EventType::FileRead => WeaverAuditEventType::FileRead,
			loom_weaver_ebpf_common::EventType::FileMetadata => WeaverAuditEventType::FileMetadata,
			loom_weaver_ebpf_common::EventType::FileOpen => WeaverAuditEventType::FileOpen,
			loom_weaver_ebpf_common::EventType::NetworkSocket => WeaverAuditEventType::NetworkSocket,
			loom_weaver_ebpf_common::EventType::NetworkConnect => WeaverAuditEventType::NetworkConnect,
			loom_weaver_ebpf_common::EventType::NetworkListen => WeaverAuditEventType::NetworkListen,
			loom_weaver_ebpf_common::EventType::NetworkAccept => WeaverAuditEventType::NetworkAccept,
			loom_weaver_ebpf_common::EventType::DnsQuery => WeaverAuditEventType::DnsQuery,
			loom_weaver_ebpf_common::EventType::DnsResponse => WeaverAuditEventType::DnsResponse,
			loom_weaver_ebpf_common::EventType::PrivilegeChange => WeaverAuditEventType::PrivilegeChange,
			loom_weaver_ebpf_common::EventType::MemoryExec => WeaverAuditEventType::MemoryExec,
			loom_weaver_ebpf_common::EventType::SandboxEscape => WeaverAuditEventType::SandboxEscape,
		}
	}
}
