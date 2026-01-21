<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Health Check System Specification

**Status:** Draft\
**Version:** 1.1\
**Last Updated:** 2025-01-18

---

## 1. Overview

### Purpose

The health check system provides endpoints for monitoring the operational status of loom-server
components. It enables load balancers to route traffic appropriately and provides operators with
diagnostic information.

### Goals

- **Simple health probes**: HTTP status code for load balancer integration
- **Component status**: Individual health status for each subsystem
- **Diagnostic info**: Latency, errors, and version information
- **Non-blocking**: Fast response times with timeouts on all checks

---

## 2. Health Status Model

### 2.1 Status Values

| Status      | Description                      | HTTP Code |
| ----------- | -------------------------------- | --------- |
| `healthy`   | All components operational       | 200       |
| `degraded`  | Non-critical components impaired | 200       |
| `unhealthy` | Critical components failed       | 503       |
| `unknown`   | Status cannot be determined      | 503       |

### 2.2 Component Classification

| Component        | Criticality  | Failure Impact                        |
| ---------------- | ------------ | ------------------------------------- |
| Database         | Critical     | `unhealthy` - service cannot function |
| Binary Directory | Non-critical | `degraded` - updates unavailable      |
| LLM Providers    | Non-critical | `degraded` - inference unavailable    |
| Google CSE       | Non-critical | `degraded` - web search unavailable   |
| Auth Providers   | Non-critical | `degraded` - some login methods unavailable |

---

## 3. Endpoint Specification

### 3.1 GET /health

Returns comprehensive health status with component details.

**Request:**

```http
GET /health HTTP/1.1
Host: loom.example.com
```

**Response (healthy):**

```json
{
	"status": "healthy",
	"timestamp": "2025-01-01T12:34:56.789Z",
	"duration_ms": 4,
	"version": {
		"git_sha": "abc1234"
	},
	"components": {
		"database": {
			"status": "healthy",
			"latency_ms": 2
		},
		"bin_dir": {
			"status": "healthy",
			"latency_ms": 0,
			"path": "./bin",
			"exists": true,
			"is_dir": true,
			"file_count": 5
		},
		"llm_providers": {
			"status": "unknown",
			"providers": []
		},
		"google_cse": {
			"status": "healthy",
			"latency_ms": 245,
			"configured": true
		}
	}
}
```

**Response (degraded):**

```json
{
	"status": "degraded",
	"timestamp": "2025-01-01T12:34:56.789Z",
	"duration_ms": 5,
	"version": {
		"git_sha": "abc1234"
	},
	"components": {
		"database": {
			"status": "healthy",
			"latency_ms": 2
		},
		"bin_dir": {
			"status": "degraded",
			"latency_ms": 0,
			"path": "./bin",
			"exists": false,
			"is_dir": false,
			"error": "binary directory does not exist"
		},
		"llm_providers": {
			"status": "unknown",
			"providers": []
		},
		"google_cse": {
			"status": "degraded",
			"latency_ms": 0,
			"configured": false,
			"error": "Google CSE not configured"
		}
	}
}
```

**Response (unhealthy):**

```json
{
	"status": "unhealthy",
	"timestamp": "2025-01-01T12:34:56.789Z",
	"duration_ms": 502,
	"version": {
		"git_sha": "abc1234"
	},
	"components": {
		"database": {
			"status": "unhealthy",
			"latency_ms": 500,
			"error": "database health check timed out"
		},
		"bin_dir": {
			"status": "healthy",
			"latency_ms": 0,
			"path": "./bin",
			"exists": true,
			"is_dir": true,
			"file_count": 5
		},
		"llm_providers": {
			"status": "unknown",
			"providers": []
		},
		"google_cse": {
			"status": "healthy",
			"latency_ms": 180,
			"configured": true
		}
	}
}
```

---

## 4. Component Checks

### 4.1 Database Check

Performs a lightweight query to verify database connectivity.

```sql
SELECT 1
```

**Timeout:** 500ms

**Status mapping:**

- Query succeeds → `healthy`
- Query fails → `unhealthy`
- Timeout → `unhealthy`

### 4.2 Binary Directory Check

Verifies the CLI binary distribution directory exists and contains files.

**Checks performed:**

1. Path exists
2. Path is a directory
3. Directory is readable
4. Directory contains files

**Status mapping:**

- Directory exists with files → `healthy`
- Directory missing → `degraded`
- Directory empty → `degraded`
- Read error → `degraded`

### 4.3 LLM Provider Check (Future)

Will verify connectivity to configured LLM provider APIs.

**Planned checks:**

- HTTP connectivity to provider base URL
- Optional: lightweight API call (e.g., list models)

**Status mapping:**

- All providers reachable → `healthy`
- Some providers unreachable → `degraded`
- All providers unreachable → `degraded` (not unhealthy, as local features still work)

### 4.4 Google CSE Check

Verifies Google Custom Search Engine configuration and connectivity.

**Timeout:** 5 seconds

**Checks performed:**

1. Environment variables configured (`LOOM_SERVER_GOOGLE_CSE_API_KEY`, `LOOM_SERVER_GOOGLE_CSE_SEARCH_ENGINE_ID`)
2. API connectivity test (lightweight search query)

**Status mapping:**

- Configured and API responds → `healthy`
- Configured but rate limited → `degraded`
- Configured but timeout → `degraded`
- Configured but auth error → `unhealthy`
- Not configured → `degraded` (CSE is optional)

### 4.5 Auth Providers Check

Validates that authentication providers (OAuth, Magic Link) are properly configured.

**Providers checked:**

1. GitHub OAuth - client ID and secret configured
2. Google OAuth - client ID and secret configured
3. Okta OAuth - domain, client ID, and secret configured
4. Magic Link - SMTP configured (required for sending emails)

**Status mapping:**

- All providers configured → `healthy`
- At least one provider configured → `healthy`
- No providers configured → `unhealthy`
- Individual unconfigured providers → `degraded` (per-provider)

---

## 5. Response Schema

### 5.1 HealthResponse

```typescript
interface HealthResponse {
	status: 'healthy' | 'degraded' | 'unhealthy' | 'unknown';
	timestamp: string; // RFC3339
	duration_ms: number; // Total check duration
	version: VersionInfo;
	components: HealthComponents;
}
```

### 5.2 VersionInfo

```typescript
interface VersionInfo {
	git_sha: string; // Git commit SHA
}
```

### 5.3 HealthComponents

```typescript
interface HealthComponents {
	database: DatabaseHealth;
	bin_dir: BinDirHealth;
	llm_providers: LlmProvidersHealth;
	google_cse: GoogleCseHealth;
	auth_providers: AuthProvidersHealth;
}
```

### 5.4 DatabaseHealth

```typescript
interface DatabaseHealth {
	status: HealthStatus;
	latency_ms: number;
	error?: string;
}
```

### 5.5 BinDirHealth

```typescript
interface BinDirHealth {
	status: HealthStatus;
	latency_ms: number;
	path: string;
	exists: boolean;
	is_dir: boolean;
	file_count?: number;
	error?: string;
}
```

### 5.6 LlmProvidersHealth

```typescript
interface LlmProvidersHealth {
	status: HealthStatus;
	providers: LlmProviderHealth[];
}

interface LlmProviderHealth {
	name: string;
	status: HealthStatus;
	latency_ms?: number;
	error?: string;
}
```

### 5.7 GoogleCseHealth

```typescript
interface GoogleCseHealth {
	status: HealthStatus;
	latency_ms: number;
	configured: boolean;
	error?: string;
}
```

### 5.8 AuthProvidersHealth

```typescript
interface AuthProvidersHealth {
	status: HealthStatus;
	providers: AuthProviderHealth[];
}

interface AuthProviderHealth {
	name: string; // "github", "google", "okta", "magic_link"
	status: HealthStatus;
	configured: boolean;
	error?: string;
}
```

---

## 6. Usage Patterns

### 6.1 Load Balancer Integration

Load balancers should use HTTP status code only:

```
Health check: GET /health
Healthy: HTTP 200
Unhealthy: HTTP 503
Interval: 10-30 seconds
Timeout: 5 seconds
```

### 6.2 Monitoring Integration

Monitoring systems can parse JSON for detailed metrics:

```bash
# Check overall status
curl -s /health | jq '.status'

# Get database latency
curl -s /health | jq '.components.database.latency_ms'

# Check for any errors
curl -s /health | jq '.components | .. | .error? // empty'
```

### 6.3 Debugging

```bash
# Full health report
curl -s /health | jq .

# Check specific component
curl -s /health | jq '.components.database'
```

---

## 7. Implementation Notes

### 7.1 Parallelization

Component checks run in parallel using `tokio::join!` to minimize total latency.

### 7.2 Timeouts

Each component check has an individual timeout to prevent slow checks from blocking the response:

- Database: 500ms
- Bin dir: 500ms (sync I/O, typically instant)
- LLM providers: 300ms per provider (future)
- Google CSE: 5 seconds

### 7.3 Error Handling

Errors are captured and reported in the `error` field rather than causing the endpoint to fail. This
ensures partial information is always available.

---

## 8. Future Considerations

### 8.1 Separate Liveness/Readiness

- `/healthz` - Liveness probe (is the process running?)
- `/readyz` - Readiness probe (is the service ready to accept traffic?)

### 8.2 Health Check Caching

For expensive remote checks (LLM providers), implement caching with TTL:

- Cache duration: 10-30 seconds
- Stale-while-revalidate pattern

### 8.3 Metrics Export

Expose health metrics in Prometheus format:

```
loom_health_status{component="database"} 1
loom_health_latency_ms{component="database"} 2
```
