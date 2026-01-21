// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

// SECURITY: All event structs MUST be fully zero-initialized before
// populating fields to prevent leaking uninitialized kernel memory.

#![no_std]
#![no_main]

use aya_ebpf::{
	macros::{map, tracepoint},
	maps::RingBuf,
	programs::TracePointContext,
};
use aya_log_ebpf::info;
use loom_weaver_ebpf::{create_event_header, get_pid_tgid, read_str_from_user, should_capture_event};
use loom_weaver_ebpf_common::{
	ConnectEvent, DnsQueryEvent, DnsResponseEvent, EscapeType, EventType, FileOpenEvent,
	MemoryExecEvent, NetworkAcceptEvent, NetworkListenEvent, NetworkSocketEvent,
	PrivilegeChangeEvent, ProcessExecEvent, ProcessExitEvent, ProcessForkEvent, SandboxEscapeEvent,
	MAX_ADDR_LEN, MAX_COMM_LEN, MAX_HOSTNAME_LEN, MAX_PATH_LEN,
};

#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

#[tracepoint]
pub fn sys_enter_execve(ctx: TracePointContext) -> u32 {
	match try_sys_enter_execve(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_execve(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::ProcessExec);

	let filename_ptr: *const u8 = unsafe { ctx.read_at(16)? };

	let mut event = ProcessExecEvent {
		header,
		filename: [0u8; MAX_PATH_LEN],
		filename_len: 0,
		ret: 0,
	};

	unsafe {
		if let Ok(len) = read_str_from_user(&ctx, filename_ptr, &mut event.filename) {
			event.filename_len = len as u32;
		}
	}

	if let Some(mut buf) = EVENTS.reserve::<ProcessExecEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "execve: pid={}", header.pid);
	Ok(())
}

#[tracepoint]
pub fn sys_exit_execve(ctx: TracePointContext) -> u32 {
	match try_sys_exit_execve(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_exit_execve(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let ret: i64 = unsafe { ctx.read_at(16)? };
	let (pid, _tgid) = get_pid_tgid();

	info!(&ctx, "execve exit: pid={} ret={}", pid, ret);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_openat(ctx: TracePointContext) -> u32 {
	match try_sys_enter_openat(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_openat(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::FileOpen);

	let dirfd: i32 = unsafe { ctx.read_at(16)? };
	let filename_ptr: *const u8 = unsafe { ctx.read_at(24)? };
	let flags: i32 = unsafe { ctx.read_at(32)? };

	let mut event = FileOpenEvent {
		header,
		dirfd,
		flags,
		filename: [0u8; MAX_PATH_LEN],
		filename_len: 0,
	};

	unsafe {
		if let Ok(len) = read_str_from_user(&ctx, filename_ptr, &mut event.filename) {
			event.filename_len = len as u32;
		}
	}

	if let Some(mut buf) = EVENTS.reserve::<FileOpenEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "openat: pid={} dirfd={} flags={}", header.pid, dirfd, flags);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_connect(ctx: TracePointContext) -> u32 {
	match try_sys_enter_connect(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_connect(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::NetworkConnect);

	let sockfd: i32 = unsafe { ctx.read_at(16)? };
	let addr_ptr: *const u8 = unsafe { ctx.read_at(24)? };
	let addrlen: u32 = unsafe { ctx.read_at(32)? };

	let mut event = ConnectEvent { header, sockfd, addrlen, addr: [0u8; 128], family: 0, port: 0 };

	let read_len = if addrlen as usize > 128 { 128 } else { addrlen as usize };

	unsafe {
		if aya_ebpf::helpers::bpf_probe_read_user_buf(
			addr_ptr as *const u8,
			&mut event.addr[..read_len],
		)
		.is_ok()
		{
			if read_len >= 2 {
				event.family = u16::from_ne_bytes([event.addr[0], event.addr[1]]);
			}
			if read_len >= 4 && (event.family == 2 || event.family == 10) {
				event.port = u16::from_be_bytes([event.addr[2], event.addr[3]]);
			}
		}
	}

	if let Some(mut buf) = EVENTS.reserve::<ConnectEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "connect: pid={} sockfd={} family={}", header.pid, sockfd, event.family);
	Ok(())
}

// =============================================================================
// Network events: socket, bind, listen, accept
// =============================================================================

#[tracepoint]
pub fn sys_enter_socket(ctx: TracePointContext) -> u32 {
	match try_sys_enter_socket(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_socket(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::NetworkSocket);

	let domain: u32 = unsafe { ctx.read_at(16)? };
	let sock_type: u32 = unsafe { ctx.read_at(24)? };
	let protocol: u32 = unsafe { ctx.read_at(32)? };

	let event = NetworkSocketEvent {
		header,
		domain,
		sock_type,
		protocol,
		fd: -1, // Will be set on exit
	};

	if let Some(mut buf) = EVENTS.reserve::<NetworkSocketEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "socket: pid={} domain={} type={}", header.pid, domain, sock_type);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_bind(ctx: TracePointContext) -> u32 {
	match try_sys_enter_bind(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_bind(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::NetworkListen);

	let sockfd: i32 = unsafe { ctx.read_at(16)? };
	let addr_ptr: *const u8 = unsafe { ctx.read_at(24)? };
	let addrlen: u32 = unsafe { ctx.read_at(32)? };

	let mut event = NetworkListenEvent {
		header,
		fd: sockfd,
		backlog: 0, // Will be set on listen()
		family: 0,
		port: 0,
	};

	let read_len = if addrlen as usize > 128 { 128 } else { addrlen as usize };
	let mut addr_buf = [0u8; 128];

	unsafe {
		if aya_ebpf::helpers::bpf_probe_read_user_buf(addr_ptr, &mut addr_buf[..read_len]).is_ok() {
			if read_len >= 2 {
				event.family = u16::from_ne_bytes([addr_buf[0], addr_buf[1]]);
			}
			// AF_INET = 2, AF_INET6 = 10
			if read_len >= 4 && (event.family == 2 || event.family == 10) {
				event.port = u16::from_be_bytes([addr_buf[2], addr_buf[3]]);
			}
		}
	}

	if let Some(mut buf) = EVENTS.reserve::<NetworkListenEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "bind: pid={} fd={} port={}", header.pid, sockfd, event.port);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_listen(ctx: TracePointContext) -> u32 {
	match try_sys_enter_listen(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_listen(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::NetworkListen);

	let sockfd: i32 = unsafe { ctx.read_at(16)? };
	let backlog: i32 = unsafe { ctx.read_at(24)? };

	let event = NetworkListenEvent {
		header,
		fd: sockfd,
		backlog,
		family: 0,
		port: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<NetworkListenEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "listen: pid={} fd={} backlog={}", header.pid, sockfd, backlog);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_accept(ctx: TracePointContext) -> u32 {
	match try_sys_enter_accept(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_accept(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::NetworkAccept);

	let sockfd: i32 = unsafe { ctx.read_at(16)? };

	let event = NetworkAcceptEvent {
		header,
		listen_fd: sockfd,
		conn_fd: -1, // Will be set on exit
		family: 0,
		port: 0,
		addr: [0u8; MAX_ADDR_LEN],
		addr_len: 0,
		_pad: [0u8; 3],
	};

	if let Some(mut buf) = EVENTS.reserve::<NetworkAcceptEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "accept: pid={} listen_fd={}", header.pid, sockfd);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_accept4(ctx: TracePointContext) -> u32 {
	match try_sys_enter_accept4(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_accept4(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::NetworkAccept);

	let sockfd: i32 = unsafe { ctx.read_at(16)? };
	let flags: i32 = unsafe { ctx.read_at(40)? };

	let event = NetworkAcceptEvent {
		header,
		listen_fd: sockfd,
		conn_fd: -1,
		family: 0,
		port: 0,
		addr: [0u8; MAX_ADDR_LEN],
		addr_len: 0,
		_pad: [0u8; 3],
	};

	if let Some(mut buf) = EVENTS.reserve::<NetworkAcceptEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "accept4: pid={} listen_fd={} flags={}", header.pid, sockfd, flags);
	Ok(())
}

// =============================================================================
// DNS detection: sendto/recvfrom to port 53
// =============================================================================

const DNS_PORT: u16 = 53;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 10;

#[tracepoint]
pub fn sys_enter_sendto(ctx: TracePointContext) -> u32 {
	match try_sys_enter_sendto(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_sendto(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}

	let sockfd: i32 = unsafe { ctx.read_at(16)? };
	let buf_ptr: *const u8 = unsafe { ctx.read_at(24)? };
	let len: u64 = unsafe { ctx.read_at(32)? };
	let dest_addr_ptr: *const u8 = unsafe { ctx.read_at(48)? };
	let addrlen: u32 = unsafe { ctx.read_at(56)? };

	// Check if this is to port 53 (DNS)
	if dest_addr_ptr.is_null() || addrlen < 4 {
		return Ok(());
	}

	let mut addr_buf = [0u8; 128];
	let read_len = if addrlen as usize > 128 { 128 } else { addrlen as usize };

	unsafe {
		if aya_ebpf::helpers::bpf_probe_read_user_buf(dest_addr_ptr, &mut addr_buf[..read_len])
			.is_err()
		{
			return Ok(());
		}
	}

	let family = u16::from_ne_bytes([addr_buf[0], addr_buf[1]]);
	if family != AF_INET && family != AF_INET6 {
		return Ok(());
	}

	let port = u16::from_be_bytes([addr_buf[2], addr_buf[3]]);
	if port != DNS_PORT {
		return Ok(());
	}

	// This is a DNS query (sendto to port 53)
	let header = create_event_header(EventType::DnsQuery);

	let mut event = DnsQueryEvent {
		header,
		hostname: [0u8; MAX_HOSTNAME_LEN],
		hostname_len: 0,
		query_type: 0,
		query_class: 0,
		transaction_id: 0,
		_pad: [0u8; 2],
	};

	// Read DNS packet header to get transaction ID
	if len >= 12 {
		let mut dns_header = [0u8; 12];
		unsafe {
			if aya_ebpf::helpers::bpf_probe_read_user_buf(buf_ptr, &mut dns_header).is_ok() {
				event.transaction_id = u16::from_be_bytes([dns_header[0], dns_header[1]]);
			}
		}
	}

	if let Some(mut buf) = EVENTS.reserve::<DnsQueryEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "dns_query: pid={} fd={} txid={}", header.pid, sockfd, event.transaction_id);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_recvfrom(ctx: TracePointContext) -> u32 {
	match try_sys_enter_recvfrom(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_recvfrom(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}

	let sockfd: i32 = unsafe { ctx.read_at(16)? };
	let src_addr_ptr: *const u8 = unsafe { ctx.read_at(40)? };
	let addrlen_ptr: *const u32 = unsafe { ctx.read_at(48)? };

	// For recvfrom, we check on exit when we have the actual source address
	// This entry handler just logs the attempt
	let (pid, _) = get_pid_tgid();
	info!(&ctx, "recvfrom entry: pid={} fd={}", pid, sockfd);
	Ok(())
}

// =============================================================================
// Process events
// =============================================================================

#[tracepoint]
pub fn sys_enter_clone(ctx: TracePointContext) -> u32 {
	match try_sys_enter_clone(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_clone(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::ProcessFork);

	let clone_flags: u64 = unsafe { ctx.read_at(16)? };

	let event = ProcessForkEvent {
		header,
		parent_pid: header.pid,
		child_pid: 0,
		clone_flags,
	};

	if let Some(mut buf) = EVENTS.reserve::<ProcessForkEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "clone: pid={} flags={}", header.pid, clone_flags);
	Ok(())
}

#[tracepoint]
pub fn sys_exit_exit_group(ctx: TracePointContext) -> u32 {
	match try_sys_exit_exit_group(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_exit_exit_group(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::ProcessExit);

	let exit_code: i32 = unsafe { ctx.read_at(16)? };

	let event = ProcessExitEvent {
		header,
		exit_code,
		signal: 0,
		comm: [0u8; MAX_COMM_LEN],
	};

	if let Some(mut buf) = EVENTS.reserve::<ProcessExitEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "exit_group: pid={} code={}", header.pid, exit_code);
	Ok(())
}

// =============================================================================
// Privilege change events
// =============================================================================

const PRIV_CHANGE_SETUID: u32 = 1;
const PRIV_CHANGE_SETGID: u32 = 2;
const PRIV_CHANGE_PTRACE: u32 = 3;
const PRIV_CHANGE_SETRESUID: u32 = 4;
const PRIV_CHANGE_SETRESGID: u32 = 5;

#[tracepoint]
pub fn sys_enter_setuid(ctx: TracePointContext) -> u32 {
	match try_sys_enter_setuid(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_setuid(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::PrivilegeChange);

	let new_uid: u32 = unsafe { ctx.read_at(16)? };

	let event = PrivilegeChangeEvent {
		header,
		old_uid: header.uid,
		new_uid,
		old_gid: header.gid,
		new_gid: header.gid,
		old_euid: header.uid,
		new_euid: new_uid,
		capability: 0,
		change_type: PRIV_CHANGE_SETUID,
	};

	if let Some(mut buf) = EVENTS.reserve::<PrivilegeChangeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "setuid: pid={} new_uid={}", header.pid, new_uid);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_setgid(ctx: TracePointContext) -> u32 {
	match try_sys_enter_setgid(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_setgid(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::PrivilegeChange);

	let new_gid: u32 = unsafe { ctx.read_at(16)? };

	let event = PrivilegeChangeEvent {
		header,
		old_uid: header.uid,
		new_uid: header.uid,
		old_gid: header.gid,
		new_gid,
		old_euid: header.uid,
		new_euid: header.uid,
		capability: 0,
		change_type: PRIV_CHANGE_SETGID,
	};

	if let Some(mut buf) = EVENTS.reserve::<PrivilegeChangeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "setgid: pid={} new_gid={}", header.pid, new_gid);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_ptrace(ctx: TracePointContext) -> u32 {
	match try_sys_enter_ptrace(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_ptrace(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::PrivilegeChange);

	let request: i64 = unsafe { ctx.read_at(16)? };
	let target_pid: i64 = unsafe { ctx.read_at(24)? };

	let event = PrivilegeChangeEvent {
		header,
		old_uid: header.uid,
		new_uid: header.uid,
		old_gid: header.gid,
		new_gid: header.gid,
		old_euid: header.uid,
		new_euid: header.uid,
		capability: target_pid as u32,
		change_type: PRIV_CHANGE_PTRACE,
	};

	if let Some(mut buf) = EVENTS.reserve::<PrivilegeChangeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "ptrace: pid={} request={} target={}", header.pid, request, target_pid);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_setresuid(ctx: TracePointContext) -> u32 {
	match try_sys_enter_setresuid(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_setresuid(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::PrivilegeChange);

	let ruid: u32 = unsafe { ctx.read_at(16)? };
	let euid: u32 = unsafe { ctx.read_at(24)? };
	let suid: u32 = unsafe { ctx.read_at(32)? };

	let event = PrivilegeChangeEvent {
		header,
		old_uid: header.uid,
		new_uid: ruid,
		old_gid: header.gid,
		new_gid: header.gid,
		old_euid: header.uid,
		new_euid: euid,
		capability: suid, // Store saved uid in capability field
		change_type: PRIV_CHANGE_SETRESUID,
	};

	if let Some(mut buf) = EVENTS.reserve::<PrivilegeChangeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "setresuid: pid={} ruid={} euid={} suid={}", header.pid, ruid, euid, suid);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_setresgid(ctx: TracePointContext) -> u32 {
	match try_sys_enter_setresgid(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_setresgid(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::PrivilegeChange);

	let rgid: u32 = unsafe { ctx.read_at(16)? };
	let egid: u32 = unsafe { ctx.read_at(24)? };
	let sgid: u32 = unsafe { ctx.read_at(32)? };

	let event = PrivilegeChangeEvent {
		header,
		old_uid: header.uid,
		new_uid: header.uid,
		old_gid: header.gid,
		new_gid: rgid,
		old_euid: header.uid,
		new_euid: header.uid,
		capability: (egid as u32) | ((sgid as u32) << 16), // Pack egid and sgid
		change_type: PRIV_CHANGE_SETRESGID,
	};

	if let Some(mut buf) = EVENTS.reserve::<PrivilegeChangeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "setresgid: pid={} rgid={} egid={} sgid={}", header.pid, rgid, egid, sgid);
	Ok(())
}

// =============================================================================
// Memory events (filtered for PROT_EXEC)
// =============================================================================

const PROT_EXEC: u32 = 0x4;

#[tracepoint]
pub fn sys_enter_mmap(ctx: TracePointContext) -> u32 {
	match try_sys_enter_mmap(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_mmap(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let addr: u64 = unsafe { ctx.read_at(16)? };
	let len: u64 = unsafe { ctx.read_at(24)? };
	let prot: u32 = unsafe { ctx.read_at(32)? };
	let flags: u32 = unsafe { ctx.read_at(40)? };
	let fd: i32 = unsafe { ctx.read_at(48)? };

	if prot & PROT_EXEC == 0 {
		return Ok(());
	}

	let header = create_event_header(EventType::MemoryExec);

	let event = MemoryExecEvent {
		header,
		addr,
		len,
		prot,
		flags,
		fd,
		path: [0u8; MAX_PATH_LEN],
		path_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<MemoryExecEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "mmap: pid={} addr={} len={} prot={}", header.pid, addr, len, prot);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_mprotect(ctx: TracePointContext) -> u32 {
	match try_sys_enter_mprotect(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_mprotect(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let addr: u64 = unsafe { ctx.read_at(16)? };
	let len: u64 = unsafe { ctx.read_at(24)? };
	let prot: u32 = unsafe { ctx.read_at(32)? };

	if prot & PROT_EXEC == 0 {
		return Ok(());
	}

	let header = create_event_header(EventType::MemoryExec);

	let event = MemoryExecEvent {
		header,
		addr,
		len,
		prot,
		flags: 0,
		fd: -1,
		path: [0u8; MAX_PATH_LEN],
		path_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<MemoryExecEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "mprotect: pid={} addr={} len={} prot={}", header.pid, addr, len, prot);
	Ok(())
}

// =============================================================================
// Sandbox escape detection (critical events)
// =============================================================================

const SYSCALL_UNSHARE: u32 = 272;
const SYSCALL_SETNS: u32 = 308;
const SYSCALL_MOUNT: u32 = 165;
const SYSCALL_UMOUNT2: u32 = 166;
const SYSCALL_INIT_MODULE: u32 = 175;
const SYSCALL_FINIT_MODULE: u32 = 313;
const SYSCALL_BPF: u32 = 321;
const SYSCALL_PERF_EVENT_OPEN: u32 = 298;

#[tracepoint]
pub fn sys_enter_unshare(ctx: TracePointContext) -> u32 {
	match try_sys_enter_unshare(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_unshare(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let flags: u64 = unsafe { ctx.read_at(16)? };

	let event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::Namespace as u32,
		syscall_nr: SYSCALL_UNSHARE,
		arg0: flags,
		arg1: 0,
		arg2: 0,
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "unshare: pid={} flags={}", header.pid, flags);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_setns(ctx: TracePointContext) -> u32 {
	match try_sys_enter_setns(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_setns(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let fd: i32 = unsafe { ctx.read_at(16)? };
	let nstype: i32 = unsafe { ctx.read_at(24)? };

	let event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::Namespace as u32,
		syscall_nr: SYSCALL_SETNS,
		arg0: fd as u64,
		arg1: nstype as u64,
		arg2: 0,
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "setns: pid={} fd={} nstype={}", header.pid, fd, nstype);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_mount(ctx: TracePointContext) -> u32 {
	match try_sys_enter_mount(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_mount(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let source_ptr: *const u8 = unsafe { ctx.read_at(16)? };
	let target_ptr: *const u8 = unsafe { ctx.read_at(24)? };
	let fstype_ptr: *const u8 = unsafe { ctx.read_at(32)? };

	let mut event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::Mount as u32,
		syscall_nr: SYSCALL_MOUNT,
		arg0: source_ptr as u64,
		arg1: target_ptr as u64,
		arg2: fstype_ptr as u64,
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	unsafe {
		if let Ok(len) = read_str_from_user(&ctx, target_ptr, &mut event.context) {
			event.context_len = len as u32;
		}
	}

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "mount: pid={}", header.pid);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_umount2(ctx: TracePointContext) -> u32 {
	match try_sys_enter_umount2(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_umount2(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let target_ptr: *const u8 = unsafe { ctx.read_at(16)? };
	let flags: i32 = unsafe { ctx.read_at(24)? };

	let mut event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::Mount as u32,
		syscall_nr: SYSCALL_UMOUNT2,
		arg0: flags as u64,
		arg1: 0,
		arg2: 0,
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	unsafe {
		if let Ok(len) = read_str_from_user(&ctx, target_ptr, &mut event.context) {
			event.context_len = len as u32;
		}
	}

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "umount2: pid={} flags={}", header.pid, flags);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_init_module(ctx: TracePointContext) -> u32 {
	match try_sys_enter_init_module(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_init_module(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let module_image_ptr: u64 = unsafe { ctx.read_at(16)? };
	let len: u64 = unsafe { ctx.read_at(24)? };
	let param_values_ptr: u64 = unsafe { ctx.read_at(32)? };

	let event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::ModuleLoad as u32,
		syscall_nr: SYSCALL_INIT_MODULE,
		arg0: module_image_ptr,
		arg1: len,
		arg2: param_values_ptr,
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "init_module: pid={} len={}", header.pid, len);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_finit_module(ctx: TracePointContext) -> u32 {
	match try_sys_enter_finit_module(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_finit_module(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let fd: i32 = unsafe { ctx.read_at(16)? };
	let param_values_ptr: u64 = unsafe { ctx.read_at(24)? };
	let flags: i32 = unsafe { ctx.read_at(32)? };

	let event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::ModuleLoad as u32,
		syscall_nr: SYSCALL_FINIT_MODULE,
		arg0: fd as u64,
		arg1: param_values_ptr,
		arg2: flags as u64,
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "finit_module: pid={} fd={} flags={}", header.pid, fd, flags);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_bpf(ctx: TracePointContext) -> u32 {
	match try_sys_enter_bpf(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_bpf(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let cmd: i32 = unsafe { ctx.read_at(16)? };
	let attr_ptr: u64 = unsafe { ctx.read_at(24)? };
	let size: u32 = unsafe { ctx.read_at(32)? };

	let event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::Bpf as u32,
		syscall_nr: SYSCALL_BPF,
		arg0: cmd as u64,
		arg1: attr_ptr,
		arg2: size as u64,
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "bpf: pid={} cmd={} size={}", header.pid, cmd, size);
	Ok(())
}

#[tracepoint]
pub fn sys_enter_perf_event_open(ctx: TracePointContext) -> u32 {
	match try_sys_enter_perf_event_open(ctx) {
		Ok(()) => 0,
		Err(_) => 1,
	}
}

fn try_sys_enter_perf_event_open(ctx: TracePointContext) -> Result<(), i64> {
	if !should_capture_event() {
		return Ok(());
	}
	let header = create_event_header(EventType::SandboxEscape);

	let attr_ptr: u64 = unsafe { ctx.read_at(16)? };
	let pid: i32 = unsafe { ctx.read_at(24)? };
	let cpu: i32 = unsafe { ctx.read_at(32)? };
	let group_fd: i32 = unsafe { ctx.read_at(40)? };
	let flags: u64 = unsafe { ctx.read_at(48)? };

	let event = SandboxEscapeEvent {
		header,
		escape_type: EscapeType::PerfEvent as u32,
		syscall_nr: SYSCALL_PERF_EVENT_OPEN,
		arg0: attr_ptr,
		arg1: ((pid as u64) << 32) | (cpu as u32 as u64),
		arg2: ((group_fd as u64) << 32) | (flags & 0xFFFFFFFF),
		context: [0u8; MAX_PATH_LEN],
		context_len: 0,
	};

	if let Some(mut buf) = EVENTS.reserve::<SandboxEscapeEvent>(0) {
		unsafe {
			buf.as_mut_ptr().write(event);
		}
		buf.submit(0);
	}

	info!(&ctx, "perf_event_open: pid={} target_pid={} cpu={}", header.pid, pid, cpu);
	Ok(())
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	loop {}
}
