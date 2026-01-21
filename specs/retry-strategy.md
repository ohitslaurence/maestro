<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# HTTP Retry Strategy

## Overview

Retry logic is critical for API reliability when interacting with external services like LLM
providers. Network conditions, rate limits, and transient server errors can cause temporary failures
that succeed on subsequent attempts. Without proper retry handling, applications would fail on
recoverable errors, leading to poor user experience and wasted compute.

The `loom-http` crate provides a generic, configurable retry mechanism with exponential
backoff and jitter, designed specifically for HTTP API clients.

## RetryConfig Structure

```rust
pub struct RetryConfig {
	pub max_attempts: u32,
	pub base_delay: Duration,
	pub max_delay: Duration,
	pub backoff_factor: f64,
	pub jitter: bool,
	pub retryable_statuses: Vec<StatusCode>,
}
```

| Field                | Type              | Default                     | Description                                 |
| -------------------- | ----------------- | --------------------------- | ------------------------------------------- |
| `max_attempts`       | `u32`             | `3`                         | Maximum number of attempts before giving up |
| `base_delay`         | `Duration`        | `200ms`                     | Initial delay before first retry            |
| `max_delay`          | `Duration`        | `5s`                        | Maximum delay between retries (cap)         |
| `backoff_factor`     | `f64`             | `2.0`                       | Multiplier for exponential growth           |
| `jitter`             | `bool`            | `true`                      | Whether to add randomness to delays         |
| `retryable_statuses` | `Vec<StatusCode>` | `[429, 408, 502, 503, 504]` | HTTP status codes to retry                  |

Reference: [crates/loom-http/src/retry.rs#L6-L32](../crates/loom-http/src/retry.rs#L6-L32)

## Exponential Backoff Algorithm

The delay between retries grows exponentially to avoid overwhelming a recovering service:

```
delay = base_delay × (backoff_factor ^ attempt)
```

### Calculation Steps

1. **Compute exponential delay**: `base_delay × backoff_factor^attempt`
2. **Apply cap**: `min(exponential_delay, max_delay)`
3. **Apply jitter** (if enabled): `capped_delay × (0.5 + random(0..1))`

This produces delays in the range `[capped_delay × 0.5, capped_delay × 1.5]`.

### Example Progression (defaults, no jitter)

| Attempt | Calculation | Delay           |
| ------- | ----------- | --------------- |
| 0       | 200ms × 2^0 | 200ms           |
| 1       | 200ms × 2^1 | 400ms           |
| 2       | 200ms × 2^2 | 800ms           |
| 3       | 200ms × 2^3 | 1600ms          |
| 4       | 200ms × 2^4 | 3200ms          |
| 5       | 200ms × 2^5 | 5000ms (capped) |

Reference: [crates/loom-http/src/retry.rs#L59-L71](../crates/loom-http/src/retry.rs#L59-L71)

## RetryableError Trait

The trait defines which errors should trigger a retry:

```rust
pub trait RetryableError {
	fn is_retryable(&self) -> bool;
}
```

### Implementation for reqwest::Error

```rust
impl RetryableError for reqwest::Error {
	fn is_retryable(&self) -> bool {
		if self.is_timeout() || self.is_connect() {
			return true;
		}

		if let Some(status) = self.status() {
			let retryable_statuses = [
				StatusCode::TOO_MANY_REQUESTS,   // 429
				StatusCode::REQUEST_TIMEOUT,     // 408
				StatusCode::INTERNAL_SERVER_ERROR, // 500
				StatusCode::BAD_GATEWAY,         // 502
				StatusCode::SERVICE_UNAVAILABLE, // 503
				StatusCode::GATEWAY_TIMEOUT,     // 504
			];
			return retryable_statuses.contains(&status);
		}

		false
	}
}
```

### Custom Error Wrapper Pattern

For custom error types, wrap the underlying error and implement `RetryableError`:

```rust
#[derive(Debug)]
pub struct ClientError {
	message: String,
	retryable: bool,
}

impl RetryableError for ClientError {
	fn is_retryable(&self) -> bool {
		self.retryable
	}
}
```

Reference: [crates/loom-http/src/retry.rs#L34-L57](../crates/loom-http/src/retry.rs#L34-L57)

## Retryable Conditions

### HTTP Status Codes

| Code | Name                  | Reason                                               |
| ---- | --------------------- | ---------------------------------------------------- |
| 408  | Request Timeout       | Client took too long; server may accept faster retry |
| 429  | Too Many Requests     | Rate limited; backoff gives quota time to reset      |
| 500  | Internal Server Error | Transient server issue may resolve                   |
| 502  | Bad Gateway           | Upstream server issue; may recover quickly           |
| 503  | Service Unavailable   | Server overloaded or in maintenance                  |
| 504  | Gateway Timeout       | Upstream timeout; may succeed on retry               |

### Network Errors

- **Timeouts**: `reqwest::Error::is_timeout()` — request exceeded deadline
- **Connection errors**: `reqwest::Error::is_connect()` — failed to establish connection

### Non-Retryable Conditions

- 400 Bad Request (client error, won't change)
- 401 Unauthorized (credentials issue)
- 403 Forbidden (permission issue)
- 404 Not Found (resource doesn't exist)
- 422 Unprocessable Entity (validation failure)

## retry() Function

Generic async retry wrapper that executes a closure with retry logic:

```rust
pub async fn retry<F, Fut, T, E>(cfg: &RetryConfig, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: RetryableError + std::fmt::Debug,
```

### Behavior

1. Execute the closure
2. On success, return immediately
3. On error:
   - If not retryable, log and return error
   - If max attempts reached, log and return error
   - Otherwise, calculate delay, log, sleep, and retry

### Structured Logging

All retry attempts are logged with context:

```rust
warn!(
		error = ?err,
		attempt = attempt,
		max_attempts = cfg.max_attempts,
		delay_ms = delay.as_millis(),
		"retrying after error"
);
```

Reference:
[crates/loom-http/src/retry.rs#L73-L119](../crates/loom-http/src/retry.rs#L73-L119)

## Design Decisions

### Why a Separate Crate?

1. **Reusability**: Both Anthropic and OpenAI clients share the same retry logic
2. **Testability**: Retry logic can be unit tested in isolation with mock errors
3. **Single responsibility**: HTTP retry concerns are separate from API-specific logic
4. **Configurability**: Each client can customize retry behavior independently

### Why Not tower-retry or backoff?

1. **Simplicity**: Our needs are specific (HTTP with jitter); general-purpose crates add complexity
2. **Control**: Direct implementation allows precise logging and error classification
3. **Minimal dependencies**: Fewer crates mean smaller binary and faster compilation
4. **Transparency**: Code is auditable and understandable in the codebase

### Jitter Importance

Without jitter, clients that fail simultaneously will retry simultaneously, causing the **thundering
herd problem**:

```
Time 0:    [Client A fails] [Client B fails] [Client C fails]
Time 200ms: [A retries]     [B retries]      [C retries]     ← Server overwhelmed again
Time 400ms: [A retries]     [B retries]      [C retries]     ← Still synchronized
```

With jitter, retries spread out:

```
Time 0:    [Client A fails] [Client B fails] [Client C fails]
Time 150ms: [A retries]
Time 250ms:                 [B retries]
Time 180ms:                                  [C retries]      ← Load distributed
```

## Configuration Per-Provider

### Anthropic Client

Uses default `RetryConfig` with standard settings:

```rust
impl AnthropicClient {
	pub fn new(config: AnthropicConfig) -> Result<Self, LlmError> {
		Ok(Self {
			config,
			http_client,
			retry_config: RetryConfig::default(), // 3 attempts, 200ms base, 5s max
		})
	}

	pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
		self.retry_config = retry_config;
		self
	}
}
```

Reference:
[crates/loom-llm-anthropic/src/client.rs#L48-L64](../crates/loom-llm-anthropic/src/client.rs#L48-L64)

### OpenAI Client

Uses customized settings with longer delays (500ms base, 30s max):

```rust
let retry_config = RetryConfig {
    max_attempts: 3,
    base_delay: Duration::from_millis(500),
    max_delay: Duration::from_secs(30),
    backoff_factor: 2.0,
    jitter: true,
    retryable_statuses: vec![
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        reqwest::StatusCode::REQUEST_TIMEOUT,
        reqwest::StatusCode::BAD_GATEWAY,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        reqwest::StatusCode::GATEWAY_TIMEOUT,
    ],
};
```

Reference:
[crates/loom-llm-openai/src/client.rs#L36-L74](../crates/loom-llm-openai/src/client.rs#L36-L74)

### Usage in Request Methods

Both clients wrap their request logic with `retry()`:

```rust
let response = retry(&self.retry_config, || {
    let req = anthropic_request_clone.clone();
    let c = client.clone();
    async move { c.send_request(&req).await }
})
.await
.map_err(LlmError::from)?;
```

The closure is invoked on each attempt, allowing fresh request construction.
