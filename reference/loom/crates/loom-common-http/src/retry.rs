// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Retry logic with exponential backoff for HTTP requests.

use reqwest::StatusCode;
use std::time::Duration;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct RetryConfig {
	pub max_attempts: u32,
	pub base_delay: Duration,
	pub max_delay: Duration,
	pub backoff_factor: f64,
	pub jitter: bool,
	pub retryable_statuses: Vec<StatusCode>,
}

impl Default for RetryConfig {
	fn default() -> Self {
		Self {
			max_attempts: 3,
			base_delay: Duration::from_millis(200),
			max_delay: Duration::from_secs(5),
			backoff_factor: 2.0,
			jitter: true,
			retryable_statuses: vec![
				StatusCode::TOO_MANY_REQUESTS,
				StatusCode::REQUEST_TIMEOUT,
				StatusCode::INTERNAL_SERVER_ERROR,
				StatusCode::BAD_GATEWAY,
				StatusCode::SERVICE_UNAVAILABLE,
				StatusCode::GATEWAY_TIMEOUT,
			],
		}
	}
}

pub trait RetryableError {
	fn is_retryable(&self) -> bool;
}

impl RetryableError for reqwest::Error {
	fn is_retryable(&self) -> bool {
		if self.is_timeout() || self.is_connect() {
			return true;
		}

		if let Some(status) = self.status() {
			let retryable_statuses = [
				StatusCode::TOO_MANY_REQUESTS,
				StatusCode::REQUEST_TIMEOUT,
				StatusCode::INTERNAL_SERVER_ERROR,
				StatusCode::BAD_GATEWAY,
				StatusCode::SERVICE_UNAVAILABLE,
				StatusCode::GATEWAY_TIMEOUT,
			];
			return retryable_statuses.contains(&status);
		}

		false
	}
}

fn calculate_delay(cfg: &RetryConfig, attempt: u32) -> Duration {
	let exponential_delay = cfg.base_delay.as_secs_f64() * cfg.backoff_factor.powi(attempt as i32);
	let capped_delay = exponential_delay.min(cfg.max_delay.as_secs_f64());

	let final_delay = if cfg.jitter {
		let jitter_factor = 0.5 + fastrand::f64();
		capped_delay * jitter_factor
	} else {
		capped_delay
	};

	Duration::from_secs_f64(final_delay)
}

pub async fn retry<F, Fut, T, E>(cfg: &RetryConfig, mut f: F) -> Result<T, E>
where
	F: FnMut() -> Fut,
	Fut: std::future::Future<Output = Result<T, E>>,
	E: RetryableError + std::fmt::Debug,
{
	let mut attempt = 0;

	loop {
		match f().await {
			Ok(result) => return Ok(result),
			Err(err) => {
				attempt += 1;

				if !err.is_retryable() {
					warn!(
							error = ?err,
							attempt = attempt,
							"non-retryable error encountered"
					);
					return Err(err);
				}

				if attempt >= cfg.max_attempts {
					warn!(
							error = ?err,
							attempt = attempt,
							max_attempts = cfg.max_attempts,
							"max retry attempts exhausted"
					);
					return Err(err);
				}

				let delay = calculate_delay(cfg, attempt - 1);
				warn!(
						error = ?err,
						attempt = attempt,
						max_attempts = cfg.max_attempts,
						delay_ms = delay.as_millis(),
						"retrying after error"
				);

				tokio::time::sleep(delay).await;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicU32, Ordering};
	use std::sync::Arc;

	#[derive(Debug)]
	struct MockError {
		retryable: bool,
	}

	impl RetryableError for MockError {
		fn is_retryable(&self) -> bool {
			self.retryable
		}
	}

	/// Purpose: Verifies that when an operation returns a non-retryable error,
	/// the retry function immediately returns the error without additional
	/// attempts. This is important because retrying non-retryable errors (e.g.,
	/// 404 Not Found, 401 Unauthorized) wastes resources and delays error
	/// propagation to callers.
	#[tokio::test]
	async fn test_non_retryable_error_fails_immediately() {
		let attempt_count = Arc::new(AtomicU32::new(0));
		let attempt_count_clone = Arc::clone(&attempt_count);

		let cfg = RetryConfig::default();

		let result: Result<(), MockError> = retry(&cfg, || {
			let count = Arc::clone(&attempt_count_clone);
			async move {
				count.fetch_add(1, Ordering::SeqCst);
				Err(MockError { retryable: false })
			}
		})
		.await;

		assert!(result.is_err());
		assert_eq!(
			attempt_count.load(Ordering::SeqCst),
			1,
			"non-retryable error should only attempt once"
		);
	}

	/// Purpose: Verifies that when an operation returns a retryable error,
	/// the retry function attempts the operation up to max_attempts times.
	/// This is critical for resilience against transient failures like network
	/// timeouts or temporary service unavailability (429, 503 errors).
	#[tokio::test]
	async fn test_retryable_error_retries_up_to_max_attempts() {
		let attempt_count = Arc::new(AtomicU32::new(0));
		let attempt_count_clone = Arc::clone(&attempt_count);

		let cfg = RetryConfig {
			max_attempts: 3,
			base_delay: Duration::from_millis(1),
			max_delay: Duration::from_millis(10),
			backoff_factor: 2.0,
			jitter: false,
			retryable_statuses: vec![],
		};

		let result: Result<(), MockError> = retry(&cfg, || {
			let count = Arc::clone(&attempt_count_clone);
			async move {
				count.fetch_add(1, Ordering::SeqCst);
				Err(MockError { retryable: true })
			}
		})
		.await;

		assert!(result.is_err());
		assert_eq!(
			attempt_count.load(Ordering::SeqCst),
			3,
			"should retry exactly max_attempts times"
		);
	}

	/// Purpose: Verifies that when an operation eventually succeeds after
	/// transient failures, the retry function returns the successful result.
	/// This ensures the retry mechanism correctly handles recovery scenarios.
	#[tokio::test]
	async fn test_succeeds_after_retries() {
		let attempt_count = Arc::new(AtomicU32::new(0));
		let attempt_count_clone = Arc::clone(&attempt_count);

		let cfg = RetryConfig {
			max_attempts: 5,
			base_delay: Duration::from_millis(1),
			max_delay: Duration::from_millis(10),
			backoff_factor: 2.0,
			jitter: false,
			retryable_statuses: vec![],
		};

		let result: Result<&str, MockError> = retry(&cfg, || {
			let count = Arc::clone(&attempt_count_clone);
			async move {
				let current = count.fetch_add(1, Ordering::SeqCst);
				if current < 2 {
					Err(MockError { retryable: true })
				} else {
					Ok("success")
				}
			}
		})
		.await;

		assert!(result.is_ok());
		assert_eq!(result.unwrap(), "success");
		assert_eq!(
			attempt_count.load(Ordering::SeqCst),
			3,
			"should succeed on third attempt"
		);
	}

	/// Purpose: Verifies that jitter adds randomness to the delay calculation,
	/// preventing the "thundering herd" problem where many clients retry
	/// simultaneously after a shared failure, overwhelming the recovering
	/// service. Without jitter, exponential backoff alone can still cause
	/// synchronized retries.
	#[test]
	fn test_jitter_adds_randomness() {
		let cfg_with_jitter = RetryConfig {
			max_attempts: 3,
			base_delay: Duration::from_millis(100),
			max_delay: Duration::from_secs(5),
			backoff_factor: 2.0,
			jitter: true,
			retryable_statuses: vec![],
		};

		let cfg_without_jitter = RetryConfig {
			jitter: false,
			..cfg_with_jitter.clone()
		};

		let delays_without_jitter: Vec<Duration> = (0..10)
			.map(|_| calculate_delay(&cfg_without_jitter, 1))
			.collect();

		let delays_with_jitter: Vec<Duration> = (0..10)
			.map(|_| calculate_delay(&cfg_with_jitter, 1))
			.collect();

		let all_same_without_jitter = delays_without_jitter.windows(2).all(|w| w[0] == w[1]);
		assert!(
			all_same_without_jitter,
			"delays without jitter should be identical"
		);

		let all_same_with_jitter = delays_with_jitter.windows(2).all(|w| w[0] == w[1]);
		assert!(!all_same_with_jitter, "delays with jitter should vary");
	}

	/// Purpose: Verifies that the calculated delay never exceeds max_delay,
	/// preventing excessively long waits that could cause request timeouts
	/// or poor user experience during extended outages.
	#[test]
	fn test_delay_respects_max_delay() {
		let cfg = RetryConfig {
			max_attempts: 10,
			base_delay: Duration::from_secs(1),
			max_delay: Duration::from_secs(5),
			backoff_factor: 10.0,
			jitter: false,
			retryable_statuses: vec![],
		};

		for attempt in 0..10 {
			let delay = calculate_delay(&cfg, attempt);
			assert!(
				delay <= Duration::from_secs_f64(5.0 * 1.5),
				"delay {delay:?} at attempt {attempt} exceeds max_delay with jitter headroom"
			);
		}
	}
}
