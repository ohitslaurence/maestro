// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#![no_std]

use aya_ebpf::{
	helpers::bpf_get_current_pid_tgid,
	macros::map,
	maps::Array,
	programs::TracePointContext,
};
use loom_weaver_ebpf_common::{EventHeader, EventType, MAX_PATH_LEN};

#[map]
pub static FILTER_ENABLED: Array<u32> = Array::with_max_entries(1, 0);

#[map]
pub static TARGET_CGROUP_ID: Array<u64> = Array::with_max_entries(1, 0);

#[inline(always)]
pub fn get_pid_tgid() -> (u32, u32) {
	let pid_tgid = bpf_get_current_pid_tgid();
	let pid = (pid_tgid >> 32) as u32;
	let tgid = pid_tgid as u32;
	(pid, tgid)
}

#[inline(always)]
pub fn current_timestamp_ns() -> u64 {
	unsafe { aya_ebpf::helpers::bpf_ktime_get_ns() }
}

#[inline(always)]
pub fn create_event_header(event_type: EventType) -> EventHeader {
	let (pid, tgid) = get_pid_tgid();
	let uid_gid = unsafe { aya_ebpf::helpers::bpf_get_current_uid_gid() };
	let uid = uid_gid as u32;
	let gid = (uid_gid >> 32) as u32;
	EventHeader {
		event_type: event_type as u32,
		timestamp_ns: current_timestamp_ns(),
		pid,
		tid: tgid,
		uid,
		gid,
	}
}

#[inline(always)]
pub unsafe fn read_str_from_user(
	ctx: &TracePointContext,
	user_ptr: *const u8,
	buf: &mut [u8; MAX_PATH_LEN],
) -> Result<usize, i64> {
	let ret = aya_ebpf::helpers::bpf_probe_read_user_str_bytes(user_ptr, buf);
	match ret {
		Ok(s) => Ok(s.len()),
		Err(e) => Err(e),
	}
}

#[inline(always)]
pub fn should_capture_event() -> bool {
	let filter_enabled = unsafe { FILTER_ENABLED.get(0) };
	match filter_enabled {
		Some(&enabled) if enabled != 0 => {
			let target_cgroup = unsafe { TARGET_CGROUP_ID.get(0) };
			match target_cgroup {
				Some(&target_id) if target_id != 0 => {
					let current_cgroup = unsafe { aya_ebpf::helpers::bpf_get_current_cgroup_id() };
					current_cgroup == target_id
				}
				_ => true,
			}
		}
		_ => true,
	}
}
