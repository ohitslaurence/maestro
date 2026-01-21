// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::types::TriggerSource;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct JobContext {
	pub run_id: String,
	pub triggered_by: TriggerSource,
	pub cancellation_token: CancellationToken,
}

#[derive(Clone)]
pub struct CancellationToken {
	cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
	pub fn new() -> Self {
		Self {
			cancelled: Arc::new(AtomicBool::new(false)),
		}
	}

	pub fn cancel(&self) {
		self.cancelled.store(true, Ordering::SeqCst);
	}

	pub fn is_cancelled(&self) -> bool {
		self.cancelled.load(Ordering::SeqCst)
	}
}

impl Default for CancellationToken {
	fn default() -> Self {
		Self::new()
	}
}
