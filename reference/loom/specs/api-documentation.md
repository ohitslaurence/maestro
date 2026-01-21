<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# API Documentation System Specification

**Status:** Approved\
**Version:** 1.0\
**Last Updated:** 2025-01-23

---

## 1. Overview

### Purpose

The API documentation system provides interactive, type-safe documentation for the loom-server HTTP
API. It generates OpenAPI 3.0 specifications directly from Rust types, ensuring documentation stays
in sync with implementation.

### Goals

- **Type-safe documentation**: Schemas derived from actual Rust types used by handlers
- **Interactive exploration**: Swagger UI for developers to discover and test endpoints
- **Code generation ready**: OpenAPI JSON spec available for client generation
- **Minimal intrusion**: No changes to existing router patterns or handler signatures
- **Future-proof**: Room for WebSocket documentation via AsyncAPI later

### Non-Goals

- Full WebSocket message schema documentation (defer to Phase 3 WebSocket implementation)
- Authentication flow documentation (auth is currently stubbed)
- Rate limiting documentation at API level

---

## 2. Technology Choice

### Selected: `utoipa` + `utoipa-swagger-ui`

| Criteria              | utoipa                                    | aide                        | Manual YAML        |
| --------------------- | ----------------------------------------- | --------------------------- | ------------------ |
| Axum integration      | ✅ Native, non-intrusive                  | ✅ Deep but requires refactor | ❌ None            |
| Type safety           | ✅ Derive from serde types                | ✅ Derive from types         | ❌ Manual sync     |
| Learning curve        | Low                                       | Medium-High                 | Low                |
| Maintenance burden    | Low (types are source of truth)           | Low                         | High (drift risk)  |
| Existing router compat| ✅ Works with plain Router                | ⚠️ Requires ApiRouter        | ✅ N/A             |

**Decision**: Use `utoipa` for its balance of type safety, low intrusion, and good Axum support.

---

## 3. Architecture

### Dependency Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                        loom-server                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │   api_docs.rs   │  │    api.rs       │  │  llm_proxy.rs  │  │
│  │   (ApiDoc)      │  │  (handlers)     │  │  (handlers)    │  │
│  └────────┬────────┘  └────────┬────────┘  └───────┬────────┘  │
│           │                    │                   │            │
│           └────────────────────┼───────────────────┘            │
│                                │                                │
│  Dependencies with feature = "openapi":                         │
│  ┌─────────────┐  ┌──────────────────┐  ┌─────────────────┐    │
│  │ loom-thread │  │ loom-github-app  │  │ loom-google-cse │    │
│  │ [openapi]   │  │ [openapi]        │  │ [openapi]       │    │
│  └─────────────┘  └──────────────────┘  └─────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### Key Principle

- `utoipa` is only a dependency of `loom-server`
- Shared crates use **optional** `openapi` feature to add `ToSchema` derives
- This keeps CLI and other consumers free of documentation dependencies

---

## 4. Endpoint Documentation Scope

### Phase 1: Core Endpoints (Initial Implementation)

| Endpoint                              | Method | Tag           | Priority |
| ------------------------------------- | ------ | ------------- | -------- |
| `/v1/threads`                         | GET    | threads       | High     |
| `/v1/threads/{id}`                    | GET    | threads       | High     |
| `/v1/threads/{id}`                    | PUT    | threads       | High     |
| `/v1/threads/{id}`                    | DELETE | threads       | High     |
| `/v1/threads/{id}/visibility`         | POST   | threads       | High     |
| `/v1/threads/search`                  | GET    | threads       | High     |
| `/health`                             | GET    | health        | High     |
| `/metrics`                            | GET    | health        | Medium   |

### Phase 2: Proxy Endpoints

| Endpoint                              | Method | Tag           | Priority |
| ------------------------------------- | ------ | ------------- | -------- |
| `/proxy/anthropic/complete`           | POST   | llm-proxy     | High     |
| `/proxy/anthropic/stream`             | POST   | llm-proxy     | High     |
| `/proxy/openai/complete`              | POST   | llm-proxy     | High     |
| `/proxy/openai/stream`                | POST   | llm-proxy     | High     |
| `/proxy/vertex/complete`              | POST   | llm-proxy     | Medium   |
| `/proxy/vertex/stream`                | POST   | llm-proxy     | Medium   |
| `/proxy/cse`                          | POST   | google-cse    | Medium   |
| `/proxy/github/search-code`           | POST   | github        | Medium   |
| `/proxy/github/repo-info`             | POST   | github        | Medium   |
| `/proxy/github/file-contents`         | POST   | github        | Medium   |

### Phase 3: Server Query & Debug Endpoints

| Endpoint                                    | Method | Tag           | Priority |
| ------------------------------------------- | ------ | ------------- | -------- |
| `/v1/sessions/{session_id}/query-response`  | POST   | server-query  | Medium   |
| `/v1/sessions/{session_id}/queries`         | GET    | server-query  | Medium   |
| `/v1/debug/query-traces`                    | GET    | debug         | Low      |
| `/v1/debug/query-traces/{trace_id}`         | GET    | debug         | Low      |
| `/v1/debug/query-traces/stats`              | GET    | debug         | Low      |

### Phase 4: GitHub App Endpoints

| Endpoint                              | Method | Tag           | Priority |
| ------------------------------------- | ------ | ------------- | -------- |
| `/v1/github/app`                      | GET    | github        | Low      |
| `/v1/github/webhook`                  | POST   | github        | Low      |
| `/v1/github/installations/by-repo`    | GET    | github        | Low      |

---

## 5. Schema Documentation

### Request/Response Types

Types requiring `ToSchema` derive:

**In `loom-server/src/api.rs`:**
- `ListParams` → `IntoParams`
- `SearchParams` → `IntoParams`
- `SearchResponse` → `ToSchema`
- `SearchResponseHit` → `ToSchema`
- `UpdateVisibilityRequest` → `ToSchema`
- `ListResponse` → `ToSchema`
- `AuthStubResponse` → `ToSchema`
- `CseProxyRequest` → `ToSchema`
- `CseProxyResponse` → `ToSchema`
- `CseProxyResultItem` → `ToSchema`
- `GithubInstallationByRepoQuery` → `IntoParams`
- `GithubSearchCodeRequest` → `ToSchema`
- `GithubRepoInfoRequest` → `ToSchema`
- `GithubFileContentsRequest` → `ToSchema`
- `GithubRepoInfoResponse` → `ToSchema`
- `GithubFileContentsResponse` → `ToSchema`

**In `loom-server/src/health.rs`:**
- `HealthResponse` → `ToSchema`
- `HealthStatus` → `ToSchema`
- `HealthComponents` → `ToSchema`
- `DatabaseHealth` → `ToSchema`
- `BinDirHealth` → `ToSchema`
- `LlmProvidersHealth` → `ToSchema`
- `GoogleCseHealth` → `ToSchema`
- `VersionInfo` → `ToSchema`

**In `loom-server/src/error.rs`:**
- `ErrorResponse` → `ToSchema` (new type for documented error format)

**In `loom-thread` (with `openapi` feature):**
- `Thread` → `ToSchema`
- `ThreadId` → `ToSchema`
- `ThreadSummary` → `ToSchema`
- `ThreadVisibility` → `ToSchema`
- `Message` → `ToSchema`
- `Role` → `ToSchema`

**In `loom-github-app` (with `openapi` feature):**
- `AppInfoResponse` → `ToSchema`
- `InstallationStatusResponse` → `ToSchema`
- `CodeSearchRequest` → `ToSchema`

**In `loom-google-cse` (with `openapi` feature):**
- `CseRequest` → `ToSchema`
- `CseResponse` → `ToSchema`

**In `loom-core` (with `openapi` feature):**
- `LlmRequest` → `ToSchema`
- `LlmResponse` → `ToSchema`

---

## 6. Implementation Details

### 6.1 Dependencies

**`crates/loom-server/Cargo.toml`:**

```toml
[dependencies]
utoipa = { version = "5", features = ["axum_extras", "uuid", "chrono"] }
utoipa-swagger-ui = { version = "8", features = ["axum"] }

# Enable openapi feature on dependent crates
loom-thread = { path = "../loom-thread", features = ["openapi"] }
loom-github-app = { path = "../loom-github-app", features = ["openapi"] }
loom-google-cse = { path = "../loom-google-cse", features = ["openapi"] }
loom-core = { path = "../loom-core", features = ["openapi"] }
```

**Shared crates pattern (`crates/loom-thread/Cargo.toml`):**

```toml
[features]
default = []
openapi = ["dep:utoipa"]

[dependencies]
utoipa = { version = "5", optional = true }
```

### 6.2 API Documentation Module

**`crates/loom-server/src/api_docs.rs`:**

```rust
//! OpenAPI documentation for loom-server.

use utoipa::OpenApi;

/// Main OpenAPI documentation struct.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Loom Server API",
        version = "1.0.0",
        description = "AI-powered coding assistant server API",
        license(name = "Proprietary"),
        contact(name = "Geoffrey Huntley", email = "ghuntley@ghuntley.com")
    ),
    servers(
        (url = "/", description = "Local server")
    ),
    tags(
        (name = "threads", description = "Thread CRUD and search operations"),
        (name = "health", description = "Health checks and metrics"),
        (name = "llm-proxy", description = "LLM provider proxy endpoints"),
        (name = "github", description = "GitHub App integration"),
        (name = "google-cse", description = "Google Custom Search proxy"),
        (name = "server-query", description = "Server query orchestration"),
        (name = "debug", description = "Debug and tracing endpoints"),
        (name = "auth", description = "Authentication (stub)")
    ),
    paths(
        // Thread endpoints
        crate::api::list_threads,
        crate::api::get_thread,
        crate::api::upsert_thread,
        crate::api::delete_thread,
        crate::api::update_thread_visibility,
        crate::api::search_threads,
        // Health endpoints
        crate::api::health_check,
        crate::api::prometheus_metrics,
        // ... additional paths
    ),
    components(
        schemas(
            // API types
            crate::api::ListParams,
            crate::api::SearchParams,
            crate::api::SearchResponse,
            // ... additional schemas
        )
    )
)]
pub struct ApiDoc;
```

### 6.3 Router Integration

**In `crates/loom-server/src/api.rs`:**

```rust
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use crate::api_docs::ApiDoc;

pub fn create_router(state: AppState) -> Router {
    // ... existing route definitions ...

    let mut router = Router::new()
        // ... all existing routes ...
        .with_state(state);

    // Add OpenAPI documentation
    router = router.merge(
        SwaggerUi::new("/api")
            .url("/api/openapi.json", ApiDoc::openapi())
    );

    // ... existing fallback service logic ...

    router
}
```

### 6.4 Endpoint Annotation Pattern

```rust
/// GET /v1/threads - List threads.
#[utoipa::path(
    get,
    path = "/v1/threads",
    params(ListParams),
    responses(
        (status = 200, description = "Successfully retrieved threads", body = ListResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "threads"
)]
async fn list_threads(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ServerError> {
    // ... existing implementation ...
}
```

### 6.5 Streaming Endpoint Documentation

For SSE/streaming endpoints:

```rust
/// POST /proxy/anthropic/stream - Stream completion from Anthropic.
#[utoipa::path(
    post,
    path = "/proxy/anthropic/stream",
    request_body = LlmRequest,
    responses(
        (status = 200, description = "SSE stream of completion events", 
         content_type = "text/event-stream"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "llm-proxy"
)]
async fn proxy_anthropic_stream(...) -> impl IntoResponse {
    // ...
}
```

---

## 7. Documentation Endpoints

| Endpoint            | Method | Description                          |
| ------------------- | ------ | ------------------------------------ |
| `/api`              | GET    | Swagger UI interactive documentation |
| `/api/openapi.json` | GET    | Raw OpenAPI 3.0 JSON specification   |

---

## 8. Error Response Schema

All endpoints document a consistent error response format:

```rust
/// Standard error response format.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}
```

---

## 9. Testing Strategy

### 9.1 Schema Sync Test

Verify error types serialize to documented format:

```rust
#[test]
fn test_error_response_matches_schema() {
    let error = ServerError::NotFound("test".to_string());
    let json = serde_json::to_value(&error.into_response()).unwrap();
    
    // Verify structure matches ErrorResponse schema
    assert!(json.get("code").is_some());
    assert!(json.get("message").is_some());
}
```

### 9.2 OpenAPI Spec Validation

```rust
#[test]
fn test_openapi_spec_valid() {
    let spec = ApiDoc::openapi();
    let json = serde_json::to_string_pretty(&spec).unwrap();
    
    // Spec should be valid JSON
    assert!(!json.is_empty());
    
    // Should have required fields
    assert!(json.contains("openapi"));
    assert!(json.contains("3.1"));
}
```

### 9.3 CI Integration

- Generate `openapi.json` during build
- Diff-check against committed spec to detect API changes
- Validate spec with `openapi-generator validate`

---

## 10. Future Considerations

### 10.1 WebSocket Documentation (Phase 3)

When WebSocket support is added:

1. Document handshake endpoint in OpenAPI:
   ```rust
   #[utoipa::path(
       get,
       path = "/v1/ws/sessions/{session_id}",
       responses((status = 101, description = "WebSocket upgrade")),
       tag = "server-query"
   )]
   ```

2. Consider AsyncAPI for message schemas if needed

### 10.2 Authentication Documentation

When auth is implemented beyond stubs:

1. Add security schemes to OpenAPI
2. Document auth flows
3. Add bearer token requirements to protected endpoints

### 10.3 Versioning

If API versioning is needed:

1. Consider `/v2/` prefix for breaking changes
2. Document version differences in spec
3. Support multiple OpenAPI specs if needed

---

## 11. Implementation Checklist

- [ ] Add `utoipa` dependencies to `loom-server/Cargo.toml`
- [ ] Add optional `openapi` feature to `loom-thread/Cargo.toml`
- [ ] Add optional `openapi` feature to `loom-github-app/Cargo.toml`
- [ ] Add optional `openapi` feature to `loom-google-cse/Cargo.toml`
- [ ] Add optional `openapi` feature to `loom-core/Cargo.toml`
- [ ] Create `api_docs.rs` module
- [ ] Add `ToSchema` derives to all request/response types
- [ ] Add `IntoParams` derives to query parameter types
- [ ] Add `#[utoipa::path]` to all endpoint handlers
- [ ] Integrate `SwaggerUi` into router
- [ ] Add `ErrorResponse` type for documented errors
- [ ] Add tests for OpenAPI spec validity
- [ ] Update README with docs endpoint information
