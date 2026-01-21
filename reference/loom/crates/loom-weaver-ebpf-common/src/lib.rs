// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#![no_std]

// Constants for buffer sizes - must be fixed for eBPF compatibility
pub const MAX_PATH_LEN: usize = 256;
pub const MAX_ARGV_LEN: usize = 256;
pub const MAX_COMM_LEN: usize = 16;
pub const MAX_HOSTNAME_LEN: usize = 256;
pub const MAX_ADDR_LEN: usize = 16; // IPv6 address length

/// Event types for eBPF audit events.
/// Uses u32 for eBPF compatibility (no enum support in older kernels).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
	ProcessExec = 1,
	ProcessFork = 2,
	ProcessExit = 3,
	FileWrite = 4,
	FileRead = 5,
	FileMetadata = 6,
	FileOpen = 7,
	NetworkSocket = 8,
	NetworkConnect = 9,
	NetworkListen = 10,
	NetworkAccept = 11,
	DnsQuery = 12,
	DnsResponse = 13,
	PrivilegeChange = 14,
	MemoryExec = 15,
	SandboxEscape = 16,
}

impl EventType {
	/// Convert from u32 to EventType, returning None for invalid values.
	#[inline]
	pub fn from_u32(value: u32) -> Option<Self> {
		match value {
			1 => Some(Self::ProcessExec),
			2 => Some(Self::ProcessFork),
			3 => Some(Self::ProcessExit),
			4 => Some(Self::FileWrite),
			5 => Some(Self::FileRead),
			6 => Some(Self::FileMetadata),
			7 => Some(Self::FileOpen),
			8 => Some(Self::NetworkSocket),
			9 => Some(Self::NetworkConnect),
			10 => Some(Self::NetworkListen),
			11 => Some(Self::NetworkAccept),
			12 => Some(Self::DnsQuery),
			13 => Some(Self::DnsResponse),
			14 => Some(Self::PrivilegeChange),
			15 => Some(Self::MemoryExec),
			16 => Some(Self::SandboxEscape),
			_ => None,
		}
	}
}

/// Common header for all eBPF events.
/// This header is present at the start of every event in the ring buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EventHeader {
	/// Type of event (see EventType enum)
	pub event_type: u32,
	/// Kernel timestamp in nanoseconds (from bpf_ktime_get_ns)
	pub timestamp_ns: u64,
	/// Process ID
	pub pid: u32,
	/// Thread ID
	pub tid: u32,
	/// User ID
	pub uid: u32,
	/// Group ID
	pub gid: u32,
}

/// Process execution event (execve/execveat syscalls).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessExecEvent {
	pub header: EventHeader,
	/// Executable filename (null-terminated, truncated to MAX_PATH_LEN)
	pub filename: [u8; MAX_PATH_LEN],
	/// Length of filename (excluding null terminator)
	pub filename_len: u32,
	/// Return value from syscall
	pub ret: i64,
}

impl Default for ProcessExecEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			filename: [0u8; MAX_PATH_LEN],
			filename_len: 0,
			ret: 0,
		}
	}
}

/// Process fork event (fork/clone/vfork syscalls).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessForkEvent {
	pub header: EventHeader,
	/// Parent process ID
	pub parent_pid: u32,
	/// Child process ID
	pub child_pid: u32,
	/// Clone flags (CLONE_* constants)
	pub clone_flags: u64,
}

/// Process exit event.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessExitEvent {
	pub header: EventHeader,
	/// Exit code
	pub exit_code: i32,
	/// Signal that caused exit (0 if normal exit)
	pub signal: u32,
	/// Process name (comm)
	pub comm: [u8; MAX_COMM_LEN],
}

impl Default for ProcessExitEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			exit_code: 0,
			signal: 0,
			comm: [0u8; MAX_COMM_LEN],
		}
	}
}

/// File operation event (read/write/open/etc).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FileEvent {
	pub header: EventHeader,
	/// File path (null-terminated, truncated to MAX_PATH_LEN)
	pub path: [u8; MAX_PATH_LEN],
	/// Length of path
	pub path_len: u32,
	/// File descriptor
	pub fd: i32,
	/// Open flags (O_RDONLY, O_WRONLY, etc.)
	pub flags: u32,
	/// File mode (for open with O_CREAT)
	pub mode: u32,
	/// Bytes read/written (for read/write events)
	pub bytes: i64,
	/// File offset
	pub offset: i64,
}

impl Default for FileEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			path: [0u8; MAX_PATH_LEN],
			path_len: 0,
			fd: 0,
			flags: 0,
			mode: 0,
			bytes: 0,
			offset: 0,
		}
	}
}

/// File open event (openat syscall).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FileOpenEvent {
	pub header: EventHeader,
	/// Directory file descriptor
	pub dirfd: i32,
	/// Open flags
	pub flags: i32,
	/// Filename (null-terminated, truncated to MAX_PATH_LEN)
	pub filename: [u8; MAX_PATH_LEN],
	/// Length of filename
	pub filename_len: u32,
}

impl Default for FileOpenEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			dirfd: 0,
			flags: 0,
			filename: [0u8; MAX_PATH_LEN],
			filename_len: 0,
		}
	}
}

/// Network connect event (simplified for main.rs compatibility).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConnectEvent {
	pub header: EventHeader,
	/// Socket file descriptor
	pub sockfd: i32,
	/// Address length
	pub addrlen: u32,
	/// Raw address bytes
	pub addr: [u8; 128],
	/// Address family
	pub family: u16,
	/// Port number
	pub port: u16,
}

impl Default for ConnectEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			sockfd: 0,
			addrlen: 0,
			addr: [0u8; 128],
			family: 0,
			port: 0,
		}
	}
}

/// Network socket creation event.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkSocketEvent {
	pub header: EventHeader,
	/// Socket domain (AF_INET, AF_INET6, AF_UNIX, etc.)
	pub domain: u32,
	/// Socket type (SOCK_STREAM, SOCK_DGRAM, etc.)
	pub sock_type: u32,
	/// Protocol (IPPROTO_TCP, IPPROTO_UDP, etc.)
	pub protocol: u32,
	/// Resulting file descriptor
	pub fd: i32,
}

/// Network connect event.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NetworkConnectEvent {
	pub header: EventHeader,
	/// Socket file descriptor
	pub fd: i32,
	/// Address family (AF_INET = 2, AF_INET6 = 10)
	pub family: u16,
	/// Port number (network byte order converted to host)
	pub port: u16,
	/// IP address bytes (4 bytes for IPv4, 16 for IPv6)
	pub addr: [u8; MAX_ADDR_LEN],
	/// Length of address (4 for IPv4, 16 for IPv6)
	pub addr_len: u8,
	/// Padding for alignment
	pub _pad: [u8; 3],
}

impl Default for NetworkConnectEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			fd: 0,
			family: 0,
			port: 0,
			addr: [0u8; MAX_ADDR_LEN],
			addr_len: 0,
			_pad: [0u8; 3],
		}
	}
}

/// Network listen event.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkListenEvent {
	pub header: EventHeader,
	/// Socket file descriptor
	pub fd: i32,
	/// Backlog size
	pub backlog: i32,
	/// Address family
	pub family: u16,
	/// Port being listened on
	pub port: u16,
}

/// Network accept event.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NetworkAcceptEvent {
	pub header: EventHeader,
	/// Listening socket file descriptor
	pub listen_fd: i32,
	/// New connection file descriptor
	pub conn_fd: i32,
	/// Address family of accepted connection
	pub family: u16,
	/// Remote port
	pub port: u16,
	/// Remote IP address
	pub addr: [u8; MAX_ADDR_LEN],
	/// Length of address
	pub addr_len: u8,
	/// Padding for alignment
	pub _pad: [u8; 3],
}

impl Default for NetworkAcceptEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			listen_fd: 0,
			conn_fd: 0,
			family: 0,
			port: 0,
			addr: [0u8; MAX_ADDR_LEN],
			addr_len: 0,
			_pad: [0u8; 3],
		}
	}
}

/// DNS query event (extracted from UDP packets to port 53).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DnsQueryEvent {
	pub header: EventHeader,
	/// Query hostname (null-terminated)
	pub hostname: [u8; MAX_HOSTNAME_LEN],
	/// Length of hostname
	pub hostname_len: u32,
	/// DNS query type (A=1, AAAA=28, etc.)
	pub query_type: u16,
	/// DNS query class (IN=1)
	pub query_class: u16,
	/// Transaction ID
	pub transaction_id: u16,
	/// Padding for alignment
	pub _pad: [u8; 2],
}

impl Default for DnsQueryEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			hostname: [0u8; MAX_HOSTNAME_LEN],
			hostname_len: 0,
			query_type: 0,
			query_class: 0,
			transaction_id: 0,
			_pad: [0u8; 2],
		}
	}
}

/// DNS response event.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DnsResponseEvent {
	pub header: EventHeader,
	/// Query hostname (null-terminated)
	pub hostname: [u8; MAX_HOSTNAME_LEN],
	/// Length of hostname
	pub hostname_len: u32,
	/// Resolved IP address (first A/AAAA record)
	pub addr: [u8; MAX_ADDR_LEN],
	/// Length of address
	pub addr_len: u8,
	/// Transaction ID
	pub transaction_id: u16,
	/// Response code (0=NOERROR, 3=NXDOMAIN, etc.)
	pub rcode: u8,
	/// TTL of the response
	pub ttl: u32,
}

impl Default for DnsResponseEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			hostname: [0u8; MAX_HOSTNAME_LEN],
			hostname_len: 0,
			addr: [0u8; MAX_ADDR_LEN],
			addr_len: 0,
			transaction_id: 0,
			rcode: 0,
			ttl: 0,
		}
	}
}

/// Privilege change event (setuid, setgid, capabilities, etc.).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PrivilegeChangeEvent {
	pub header: EventHeader,
	/// Old user ID
	pub old_uid: u32,
	/// New user ID
	pub new_uid: u32,
	/// Old group ID
	pub old_gid: u32,
	/// New group ID
	pub new_gid: u32,
	/// Old effective user ID
	pub old_euid: u32,
	/// New effective user ID
	pub new_euid: u32,
	/// Capability being changed (CAP_* constant, or 0 if not applicable)
	pub capability: u32,
	/// Type of privilege change (1=setuid, 2=setgid, 3=setreuid, etc.)
	pub change_type: u32,
}

/// Memory execution event (mmap/mprotect with PROT_EXEC).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryExecEvent {
	pub header: EventHeader,
	/// Memory address
	pub addr: u64,
	/// Length of mapping
	pub len: u64,
	/// Protection flags (PROT_READ, PROT_WRITE, PROT_EXEC)
	pub prot: u32,
	/// Mapping flags (MAP_PRIVATE, MAP_ANONYMOUS, etc.)
	pub flags: u32,
	/// File descriptor (if file-backed, -1 otherwise)
	pub fd: i32,
	/// File path (if file-backed)
	pub path: [u8; MAX_PATH_LEN],
	/// Length of path
	pub path_len: u32,
}

impl Default for MemoryExecEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			addr: 0,
			len: 0,
			prot: 0,
			flags: 0,
			fd: 0,
			path: [0u8; MAX_PATH_LEN],
			path_len: 0,
		}
	}
}

/// Sandbox escape attempt detection event.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SandboxEscapeEvent {
	pub header: EventHeader,
	/// Type of escape attempt (1=namespace, 2=seccomp, 3=ptrace, etc.)
	pub escape_type: u32,
	/// Syscall number that triggered detection
	pub syscall_nr: u32,
	/// Additional context (syscall-specific)
	pub arg0: u64,
	pub arg1: u64,
	pub arg2: u64,
	/// Description/path if applicable
	pub context: [u8; MAX_PATH_LEN],
	/// Length of context
	pub context_len: u32,
}

impl Default for SandboxEscapeEvent {
	fn default() -> Self {
		Self {
			header: EventHeader::default(),
			escape_type: 0,
			syscall_nr: 0,
			arg0: 0,
			arg1: 0,
			arg2: 0,
			context: [0u8; MAX_PATH_LEN],
			context_len: 0,
		}
	}
}

/// Escape type constants for SandboxEscapeEvent.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeType {
	/// Attempt to create/enter new namespace
	Namespace = 1,
	/// Attempt to modify seccomp filters
	Seccomp = 2,
	/// Attempt to ptrace another process
	Ptrace = 3,
	/// Attempt to load kernel module
	ModuleLoad = 4,
	/// Attempt to mount filesystem
	Mount = 5,
	/// Attempt to access /proc or /sys in suspicious way
	ProcSys = 6,
	/// Attempt to use container escape techniques
	Container = 7,
	/// Attempt to use bpf syscall
	Bpf = 8,
	/// Attempt to use perf_event_open
	PerfEvent = 9,
}

impl EscapeType {
	#[inline]
	pub fn from_u32(value: u32) -> Option<Self> {
		match value {
			1 => Some(Self::Namespace),
			2 => Some(Self::Seccomp),
			3 => Some(Self::Ptrace),
			4 => Some(Self::ModuleLoad),
			5 => Some(Self::Mount),
			6 => Some(Self::ProcSys),
			7 => Some(Self::Container),
			8 => Some(Self::Bpf),
			9 => Some(Self::PerfEvent),
			_ => None,
		}
	}
}

// Compile-time size assertions to catch layout changes at build time
const _: () = {
	assert!(core::mem::size_of::<EventHeader>() == 32);
	assert!(core::mem::size_of::<ProcessExecEvent>() == 304);
	assert!(core::mem::size_of::<ProcessForkEvent>() == 48);
	assert!(core::mem::size_of::<ProcessExitEvent>() == 56);
	assert!(core::mem::size_of::<FileEvent>() == 320);
	assert!(core::mem::size_of::<FileOpenEvent>() == 304);
	assert!(core::mem::size_of::<ConnectEvent>() == 176);
	assert!(core::mem::size_of::<NetworkSocketEvent>() == 48);
	assert!(core::mem::size_of::<NetworkConnectEvent>() == 64);
	assert!(core::mem::size_of::<NetworkListenEvent>() == 48);
	assert!(core::mem::size_of::<NetworkAcceptEvent>() == 64);
	assert!(core::mem::size_of::<DnsQueryEvent>() == 304);
	assert!(core::mem::size_of::<DnsResponseEvent>() == 320);
	assert!(core::mem::size_of::<PrivilegeChangeEvent>() == 64);
	assert!(core::mem::size_of::<MemoryExecEvent>() == 320);
	assert!(core::mem::size_of::<SandboxEscapeEvent>() == 328);
};

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_event_type_round_trip() {
		for i in 1..=16 {
			let event_type = EventType::from_u32(i).unwrap();
			assert_eq!(event_type as u32, i);
		}
		assert!(EventType::from_u32(0).is_none());
		assert!(EventType::from_u32(17).is_none());
	}

	#[test]
	fn test_escape_type_round_trip() {
		for i in 1..=9 {
			let escape_type = EscapeType::from_u32(i).unwrap();
			assert_eq!(escape_type as u32, i);
		}
		assert!(EscapeType::from_u32(0).is_none());
		assert!(EscapeType::from_u32(10).is_none());
	}

	#[test]
	fn test_struct_sizes_are_exact() {
		use core::mem::size_of;

		assert_eq!(size_of::<EventHeader>(), 32);
		assert_eq!(size_of::<ProcessExecEvent>(), 304);
		assert_eq!(size_of::<ProcessForkEvent>(), 48);
		assert_eq!(size_of::<ProcessExitEvent>(), 56);
		assert_eq!(size_of::<FileEvent>(), 320);
		assert_eq!(size_of::<FileOpenEvent>(), 304);
		assert_eq!(size_of::<ConnectEvent>(), 176);
		assert_eq!(size_of::<NetworkSocketEvent>(), 48);
		assert_eq!(size_of::<NetworkConnectEvent>(), 64);
		assert_eq!(size_of::<NetworkListenEvent>(), 48);
		assert_eq!(size_of::<NetworkAcceptEvent>(), 64);
		assert_eq!(size_of::<DnsQueryEvent>(), 304);
		assert_eq!(size_of::<DnsResponseEvent>(), 320);
		assert_eq!(size_of::<PrivilegeChangeEvent>(), 64);
		assert_eq!(size_of::<MemoryExecEvent>(), 320);
		assert_eq!(size_of::<SandboxEscapeEvent>(), 328);
	}

	#[test]
	fn test_struct_alignment() {
		use core::mem::align_of;

		assert_eq!(align_of::<EventHeader>(), 8);
		assert_eq!(align_of::<ProcessExecEvent>(), 8);
		assert_eq!(align_of::<ProcessForkEvent>(), 8);
		assert_eq!(align_of::<ProcessExitEvent>(), 8);
		assert_eq!(align_of::<FileEvent>(), 8);
		assert_eq!(align_of::<FileOpenEvent>(), 8);
		assert_eq!(align_of::<ConnectEvent>(), 8);
		assert_eq!(align_of::<NetworkSocketEvent>(), 8);
		assert_eq!(align_of::<NetworkConnectEvent>(), 8);
		assert_eq!(align_of::<NetworkListenEvent>(), 8);
		assert_eq!(align_of::<NetworkAcceptEvent>(), 8);
		assert_eq!(align_of::<DnsQueryEvent>(), 8);
		assert_eq!(align_of::<DnsResponseEvent>(), 8);
		assert_eq!(align_of::<PrivilegeChangeEvent>(), 8);
		assert_eq!(align_of::<MemoryExecEvent>(), 8);
		assert_eq!(align_of::<SandboxEscapeEvent>(), 8);
	}

	#[test]
	fn test_default_implementations() {
		let _header = EventHeader::default();
		let _exec = ProcessExecEvent::default();
		let _fork = ProcessForkEvent::default();
		let _exit = ProcessExitEvent::default();
		let _file = FileEvent::default();
		let _file_open = FileOpenEvent::default();
		let _connect = ConnectEvent::default();
		let _socket = NetworkSocketEvent::default();
		let _net_connect = NetworkConnectEvent::default();
		let _listen = NetworkListenEvent::default();
		let _accept = NetworkAcceptEvent::default();
		let _dns_query = DnsQueryEvent::default();
		let _dns_response = DnsResponseEvent::default();
		let _priv_change = PrivilegeChangeEvent::default();
		let _mem_exec = MemoryExecEvent::default();
		let _sandbox = SandboxEscapeEvent::default();
	}
}
