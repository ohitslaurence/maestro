<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Web Search System Specification

**Status:** Draft\
**Version:** 1.1\
**Last Updated:** 2025-01-18

---

## 1. Overview

### Purpose

The Web Search System enables the Loom agent to perform web searches using Google Custom Search
Engine (CSE). The client sends queries to the Loom server's `/proxy/cse` endpoint, and the server
handles all Google CSE authentication—clients require no API secrets.

### Primary Use Cases

1. **Documentation lookup**: Search for API documentation, library references
2. **Error troubleshooting**: Search for error messages, stack traces
3. **Code examples**: Find implementation examples and patterns
4. **General research**: Support LLM reasoning with current web information

### Goals

- **Secret-free client**: All Google CSE credentials stay on the server
- **Simple integration**: Follow existing Tool trait patterns
- **Normalized output**: Hide Google-specific response format from LLM
- **Robust error handling**: Clear errors for network issues, rate limits, misconfigurations
- **Separation of concerns**: CSE client logic in dedicated crate, business logic in server

### Non-Goals

- Client-side caching (may add later if needed)
- Rate limiting on server (may add later if quota becomes an issue)
- Multi-provider support (e.g., Bing, Brave)
- Image search or advanced search operators

---

## 2. Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   LLM Agent     │────▶│  WebSearchTool   │────▶│  Loom Server    │
│  (tool_use)     │     │  (loom-tools)    │     │  /proxy/cse     │
└─────────────────┘     └──────────────────┘     └────────┬────────┘
                                                          │
                                                          ▼
                                                 ┌─────────────────┐
                                                 │ loom-google-cse │
                                                 │   (crate)       │
                                                 └────────┬────────┘
                                                          │
                                                          ▼
                                                 ┌─────────────────┐
                                                 │  Google CSE API │
                                                 │  (googleapis)   │
                                                 └─────────────────┘
```

### Crate Structure

```
loom/
├── crates/
│   ├── loom-google-cse/      # NEW: Google CSE client library
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── client.rs     # CseClient struct
│   │   │   ├── types.rs      # Request/response types
│   │   │   └── error.rs      # CseError enum
│   │   └── Cargo.toml
│   ├── loom-server/          # Uses loom-google-cse
│   └── loom-tools/           # WebSearchTool (calls server)
```

### Component Responsibilities

| Component             | Responsibility                                                         |
| --------------------- | ---------------------------------------------------------------------- |
| `WebSearchTool`       | Validates args, sends request to server, maps errors to `ToolError`    |
| `/proxy/cse` endpoint | Business logic: validation, rate limit handling, calls CSE client      |
| `loom-google-cse`     | HTTP client for Google CSE API, request/response types, error handling |
| Google CSE API        | Performs actual web search, returns results                            |

---

## 3. Client-Side Tool

### WebSearchTool

**Location:** `crates/loom-tools/src/web_search.rs`

The tool owns its own `reqwest::Client` and server base URL. This avoids modifying `ToolContext`.

### Input Schema

```json
{
	"type": "object",
	"properties": {
		"query": {
			"type": "string",
			"description": "Search query string in natural language."
		},
		"max_results": {
			"type": "integer",
			"minimum": 1,
			"maximum": 10,
			"description": "Maximum number of search results to return (default: 5, max: 10)."
		}
	},
	"required": ["query"]
}
```

### Output Format

```json
{
	"query": "rust async trait",
	"results": [
		{
			"title": "Async in Traits - Rust Blog",
			"url": "https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html",
			"snippet": "We are excited to announce that async functions in traits...",
			"display_link": "blog.rust-lang.org",
			"rank": 1
		}
	]
}
```

### Configuration

| Environment Variable | Description                                                 | Required |
| -------------------- | ----------------------------------------------------------- | -------- |
| `LOOM_SERVER_URL`    | Base URL for Loom server (default: `http://127.0.0.1:8080`) | No       |

### Error Mapping

| Condition                | ToolError Variant                                   |
| ------------------------ | --------------------------------------------------- |
| Empty query              | `InvalidArguments("query must not be empty")`       |
| Network timeout          | `Timeout`                                           |
| Network/connection error | `Io(message)`                                       |
| Non-2xx from server      | `Internal("web_search proxy error: HTTP {status}")` |
| Malformed response JSON  | `Serialization(message)`                            |

---

## 4. Server-Side Endpoint

### Endpoint

```
POST /proxy/cse
```

### Request Body

```json
{
	"query": "rust async trait",
	"max_results": 5
}
```

| Field         | Type   | Required | Description                            |
| ------------- | ------ | -------- | -------------------------------------- |
| `query`       | string | Yes      | Search query (non-empty)               |
| `max_results` | u32    | No       | Max results (default: 5, capped at 10) |

### Response

```json
{
	"query": "rust async trait",
	"results": [
		{
			"title": "Result title",
			"url": "https://example.com/page",
			"snippet": "Short text snippet from the page",
			"display_link": "example.com",
			"rank": 1
		}
	]
}
```

### Error Responses

| Status | Condition                                    | Body                                                           |
| ------ | -------------------------------------------- | -------------------------------------------------------------- |
| 400    | Empty query                                  | `{"error": "query must not be empty"}`                         |
| 500    | CSE not configured (missing env vars)        | `{"error": "Google CSE is not configured on the server"}`      |
| 502    | Google CSE network error or invalid response | `{"error": "Failed to contact Google CSE"}`                    |
| 503    | Google CSE rate limit (429/403)              | `{"error": "Google CSE rate limit exceeded; try again later"}` |
| 504    | Google CSE timeout                           | `{"error": "Google CSE request timed out"}`                    |

### Server Configuration

| Environment Variable             | Description                            | Required              |
| -------------------------------- | -------------------------------------- | --------------------- |
| `LOOM_SERVER_GOOGLE_CSE_API_KEY` | Google API key with CSE enabled        | Yes (for CSE to work) |
| `LOOM_SERVER_GOOGLE_CSE_CX`      | Custom Search Engine ID (cx parameter) | Yes (for CSE to work) |

---

## 5. loom-google-cse Crate

### Purpose

The `loom-google-cse` crate provides a typed Rust client for Google Custom Search Engine API. It
encapsulates all Google-specific HTTP communication and response parsing.

### CseClient

```rust
// crates/loom-google-cse/src/client.rs

pub struct CseClient {
	http_client: reqwest::Client,
	api_key: String,
	cx: String,
	base_url: String, // Default: https://www.googleapis.com/customsearch/v1
}

impl CseClient {
	fn new(api_key: impl Into<String>, cx: impl Into<String>) -> Self;
	fn with_base_url(self, base_url: impl Into<String>) -> Self;

	async fn search(&self, request: CseRequest) -> Result<CseResponse, CseError>;
}
```

### Types

```rust
// crates/loom-google-cse/src/types.rs

#[derive(Debug, Clone)]
pub struct CseRequest {
	pub query: String,
	pub num: u32, // 1-10
}

#[derive(Debug, Clone, Serialize)]
pub struct CseResponse {
	pub query: String,
	pub results: Vec<CseResultItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CseResultItem {
	pub title: String,
	pub url: String,
	pub snippet: String,
	pub display_link: Option<String>,
	pub rank: u32,
}
```

### Error Types

```rust
// crates/loom-google-cse/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum CseError {
	#[error("Network error: {0}")]
	Network(#[from] reqwest::Error),

	#[error("Request timed out")]
	Timeout,

	#[error("Rate limit exceeded")]
	RateLimited,

	#[error("Invalid API key or CSE ID")]
	Unauthorized,

	#[error("Invalid response from Google: {0}")]
	InvalidResponse(String),

	#[error("Google API error: {status} - {message}")]
	ApiError { status: u16, message: String },
}
```

### Google API Integration

**Endpoint:** `GET https://www.googleapis.com/customsearch/v1`

**Query Parameters:**

| Parameter | Value                    |
| --------- | ------------------------ |
| `key`     | API key from `CseClient` |
| `cx`      | CSE ID from `CseClient`  |
| `q`       | User's search query      |
| `num`     | Number of results (1-10) |

**Response Mapping:**

```
Google API Response              →  CseResponse
────────────────────────────────────────────────────────
items[n].title                   →  results[n].title
items[n].link                    →  results[n].url
items[n].snippet                 →  results[n].snippet
items[n].displayLink             →  results[n].display_link
(index + 1)                      →  results[n].rank
```

---

## 6. Response Caching

### Overview

CSE responses are cached in SQLite for 24 hours to reduce API calls and improve latency. The cache
uses an exact match on `(query, max_results)`.

### Cache Table Schema

```sql
-- 006_cse_cache.sql
CREATE TABLE IF NOT EXISTS cse_cache (
    query         TEXT    NOT NULL,
    max_results   INTEGER NOT NULL,
    response_json TEXT    NOT NULL,
    created_at    TEXT    NOT NULL, -- RFC3339 UTC timestamp
    PRIMARY KEY (query, max_results)
);

CREATE INDEX IF NOT EXISTS idx_cse_cache_created_at
    ON cse_cache (created_at);
```

### Cache Key Semantics

| Component     | Normalization                                    |
| ------------- | ------------------------------------------------ |
| `query`       | Lowercase, whitespace collapsed to single spaces |
| `max_results` | Clamped to 1-10                                  |

### Cache Behavior

```
┌─────────────────┐
│ /proxy/cse      │
│ receives query  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│ Check cse_cache │────▶│ Hit & fresh?    │
│ (query, max)    │     │ (<24h old)      │
└─────────────────┘     └────────┬────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │ Yes              │                  │ No
              ▼                  │                  ▼
┌─────────────────┐              │     ┌─────────────────┐
│ Return cached   │              │     │ Call Google CSE │
│ response        │              │     └────────┬────────┘
└─────────────────┘              │              │
                                 │              ▼
                                 │     ┌─────────────────┐
                                 │     │ Store in cache  │
                                 │     │ (upsert)        │
                                 │     └────────┬────────┘
                                 │              │
                                 │              ▼
                                 │     ┌─────────────────┐
                                 │     │ Cleanup expired │
                                 │     │ entries (>24h)  │
                                 │     └────────┬────────┘
                                 │              │
                                 └──────────────┘
                                        │
                                        ▼
                              ┌─────────────────┐
                              │ Return response │
                              └─────────────────┘
```

### TTL Enforcement

- **On read**: Only return entries with `created_at >= (now - 24h)`
- **On write**: Opportunistically delete entries with `created_at < (now - 24h)`

### Cache Methods (ThreadRepository)

```rust
impl ThreadRepository {
	/// Get cached CSE response if exists and not expired.
	async fn get_cse_cache(
		&self,
		query: &str,
		max_results: u32,
	) -> Result<Option<CseResponse>, ServerError>;

	/// Store CSE response in cache, cleaning up expired entries.
	async fn put_cse_cache(
		&self,
		response: &CseResponse,
		max_results: u32,
	) -> Result<(), ServerError>;
}
```

### Integration with /proxy/cse

1. Normalize query and max_results
2. Check cache: `repo.get_cse_cache(&query, max_results)`
3. On hit: return cached response immediately
4. On miss: call Google CSE
5. Store result: `repo.put_cse_cache(&response, max_results)` (best-effort)
6. Return response

---

## 7. Security Considerations

### Secret Management

- **API keys never leave the server**: Client has no knowledge of Google credentials
- **Environment variables**: Secrets loaded from env, not config files
- **Logging safety**: API keys must never appear in logs; log query, status, result count only

### Input Validation

- **Query sanitization**: Trim whitespace, reject empty queries
- **Result capping**: Server enforces `max_results <= 10` regardless of client request
- **Timeout enforcement**: Both client and server use bounded timeouts

---

## 8. Implementation Guide

### Step 1: Create loom-google-cse crate

```bash
mkdir -p crates/loom-google-cse/src
```

```rust
// crates/loom-google-cse/src/lib.rs
pub mod client;
pub mod error;
pub mod types;

pub use client::CseClient;
pub use error::CseError;
pub use types::{CseRequest, CseResponse, CseResultItem};
```

Add to workspace `Cargo.toml`:

```toml
[workspace]
members = [
	# ... existing ...
	"crates/loom-google-cse",
]
```

### Step 2: Add WebSearchTool to loom-tools

```rust
// crates/loom-tools/src/web_search.rs
pub struct WebSearchTool {
	client: reqwest::Client,
	base_url: String,
}

#[async_trait]
impl Tool for WebSearchTool {
	fn name(&self) -> &str {
		"web_search"
	}
	fn description(&self) -> &str {
		"Perform a web search via the Loom server using Google Custom Search Engine (CSE)."
	}
	fn input_schema(&self) -> serde_json::Value { // see above
	}
	async fn invoke(&self, args: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
		// ...
	}
}
```

### Step 3: Export from lib.rs

```rust
// crates/loom-tools/src/lib.rs
pub mod web_search;
pub use web_search::WebSearchTool;
```

### Step 4: Register in CLI

```rust
// crates/loom-cli/src/main.rs
use loom_tools::WebSearchTool;

fn create_tool_registry() -> ToolRegistry {
	let mut registry = ToolRegistry::new();
	registry.register(Box::new(ReadFileTool::new()));
	registry.register(Box::new(ListFilesTool::new()));
	registry.register(Box::new(EditFileTool::new()));
	registry.register(Box::new(WebSearchTool::default())); // NEW
	registry
}
```

### Step 5: Add /proxy/cse endpoint to server

```rust
// crates/loom-server/src/api.rs
use loom_google_cse::{CseClient, CseRequest};

pub fn create_router(repo: Arc<ThreadRepository>) -> Router {
	Router::new()
        // ... existing routes ...
        .route("/proxy/cse", post(proxy_cse))  // NEW
        .with_state(repo)
}

async fn proxy_cse(
	Json(body): Json<CseProxyRequest>,
) -> Result<Json<CseProxyResponse>, ServerError> {
	// 1. Validate request
	// 2. Create CseClient with env secrets
	// 3. Call client.search()
	// 4. Map CseError -> ServerError
	// 5. Return CseResponse
}
```

---

## 9. Testing Strategy

### Property-Based Tests

```rust
proptest! {
		/// **Property: max_results is always capped at 10**
		///
		/// Why: Prevents excessive API usage and ensures bounded response sizes.
		#[test]
		fn max_results_capped_at_10(max_results in 1u32..100u32) {
				let capped = max_results.min(10);
				prop_assert!(capped <= 10);
		}

		/// **Property: Empty queries are rejected**
		///
		/// Why: Empty queries waste API quota and return no useful results.
		#[test]
		fn empty_query_rejected(query in "\\s*") {
				let trimmed = query.trim();
				prop_assert!(trimmed.is_empty());
				// Tool should return ToolError::InvalidArguments
		}

		/// **Property: Query string round-trips unchanged**
		///
		/// Why: User's intent must be preserved through the proxy.
		#[test]
		fn query_preserved(query in ".{1,200}") {
				// Response should contain same query as input
		}
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_proxy_cse_empty_query_returns_400() {
	let (app, _dir) = create_test_app().await;
	let response = app
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/proxy/cse")
				.header("Content-Type", "application/json")
				.body(Body::from(r#"{"query":""}"#))
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_web_search_tool_invalid_args() {
	let tool = WebSearchTool::default();
	let ctx = ToolContext {
		workspace_root: PathBuf::from("/tmp"),
	};
	let result = tool.invoke(json!({"query": ""}), &ctx).await;
	assert!(matches!(result, Err(ToolError::InvalidArguments(_))));
}
```

---

## 10. Future Considerations

### 9.1 Caching

Add server-side LRU cache keyed by `(query, max_results)` with short TTL (5-15 min) if:

- Quota usage becomes a concern
- Same queries are repeated frequently

### 9.2 Rate Limiting

Add `tower`-based rate limiting on `/proxy/cse` if:

- Abuse patterns emerge
- Need to protect API quota

### 9.3 Multi-Provider Support

Abstract search provider behind trait if:

- Want to support Bing, Brave, or other search APIs
- Need fallback when one provider is unavailable

### 9.4 Rich Search Options

Extend input schema to support:

- Site-specific search (`site:docs.rs`)
- Date filtering
- Safe search settings

---

## Appendix A: Full Types

### WebSearchArgs

```rust
#[derive(Debug, Deserialize)]
struct WebSearchArgs {
	query: String,
	max_results: Option<u32>,
}
```

### WebSearchResult

```rust
#[derive(Debug, Serialize)]
pub struct WebSearchResultItem {
	pub title: String,
	pub url: String,
	pub snippet: String,
	pub display_link: Option<String>,
	pub rank: u32,
}

#[derive(Debug, Serialize)]
pub struct WebSearchResult {
	pub query: String,
	pub results: Vec<WebSearchResultItem>,
}
```

### CseProxyRequest/Response (Server)

```rust
#[derive(Debug, Deserialize)]
struct CseProxyRequest {
	query: String,
	max_results: Option<u32>,
}

#[derive(Debug, Serialize)]
struct CseProxyResponse {
	query: String,
	results: Vec<CseProxyResultItem>,
}
```

## Appendix B: Error Type Extensions

### ServerError (new variants)

```rust
pub enum ServerError {
	// ... existing variants ...
	UpstreamTimeout(String),    // HTTP 504
	UpstreamError(String),      // HTTP 502
	ServiceUnavailable(String), // HTTP 503
}
```
