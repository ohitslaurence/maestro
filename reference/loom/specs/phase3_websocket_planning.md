<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Phase 3: WebSocket Architecture & Migration Plan

## Executive Summary

**Objective**: Replace SSE + HTTP polling with a single WebSocket connection for all server-client
communication, reducing query latency by 10-30x.

**Current System**: SSE for LLM streaming + HTTP POST for query responses + HTTP polling for query
delivery = 100-500ms latency\
**Phase 3 Target**: Single WebSocket connection = <50ms latency\
**Timeline**: 4-6 weeks for full implementation and load testing\
**Resource**: 1-2 engineers, staggered rollout with feature flags

---

## 1. Current Architecture (Phases 1-2)

### Transport Stack

```
┌─────────────────┐
│   LLM Client    │
└────────┬────────┘
         │
         ├─ GET /v1/sessions/{id}/llm (SSE) ──► LLM events
         │
         ├─ GET /v1/sessions/{id}/queries (HTTP polling) ──► Server queries
         │
         └─ POST /v1/sessions/{id}/query-response (HTTP) ──► Query responses
                 │
                 ├─ TCP connection overhead per request
                 ├─ 100-500ms polling latency
                 ├─ No backpressure mechanism
                 └─ Multiple concurrent connections
```

### Latency Breakdown (Current)

| Component        | Latency       | Notes               |
| ---------------- | ------------- | ------------------- |
| Query generation | 10-50ms       | Server side         |
| HTTP round-trip  | 5-20ms        | Network + TCP       |
| Polling interval | 100-500ms     | Worst case          |
| Response sending | 5-20ms        | Network + TCP       |
| **Total**        | **120-590ms** | Mostly polling wait |

---

## 2. Phase 3 Architecture (WebSocket)

### Protocol Overview

```
┌─────────────────┐
│   LLM Client    │
└────────┬────────┘
         │
         ▼
    GET /v1/ws/sessions/{id}
         │
         ├─ Upgrade to WebSocket
         │
         ▼
    Single persistent connection
         │
         ├─ ServerQuery events (server → client)
         ├─ QueryResponse messages (client → server)
         ├─ LlmStreamEvent messages (server → client)
         ├─ Control frames (keepalive, ack)
         └─ Error/reconnection logic
```

### Benefits

1. **Latency**: Reduce from 100-500ms to <50ms
   - No polling overhead
   - Immediate message delivery
   - Persistent connection reuse

2. **Resource Efficiency**
   - Single TCP connection per session
   - No connection churn overhead
   - Reduced context switches

3. **Bidirectional Communication**
   - Server pushes to client without polling
   - Client sends responses immediately
   - Symmetric latency profile

4. **Backpressure**
   - WebSocket frame buffering (OS-level)
   - Sender knows when receiver ready
   - Flow control via frame fragmentation

5. **Unified Protocol**
   - Single message format JSON
   - Type-tagged messages
   - Consistent error handling

---

## 3. Protocol Definition

### Message Format

```rust
// All messages are JSON with type tag
{
  "type": "server_query|llm_event|query_response|control|ack",
  "id": "Q-xxx|E-xxx|C-xxx", // Query/Event/Control ID
  "data": {...},              // Type-specific payload
  "timestamp": "2025-01-15T10:30:00Z"
}
```

### Message Types

#### 1. ServerQuery (server → client)

```json
{
	"type": "server_query",
	"id": "Q-0123456789abcdef0123456789abcdef",
	"data": {
		"kind": { "type": "read_file", "path": "/src/main.rs" },
		"sent_at": "2025-01-15T10:30:00Z",
		"timeout_secs": 30,
		"metadata": {}
	},
	"timestamp": "2025-01-15T10:30:00Z"
}
```

#### 2. QueryResponse (client → server)

```json
{
	"type": "query_response",
	"id": "Q-0123456789abcdef0123456789abcdef",
	"data": {
		"result": { "type": "file_content", "content": "..." },
		"error": null,
		"responded_at": "2025-01-15T10:30:00.050Z"
	},
	"timestamp": "2025-01-15T10:30:00Z"
}
```

#### 3. LlmEvent (server → client)

```json
{
	"type": "llm_event",
	"id": "E-llm-stream-001",
	"data": {
		"event_type": "text_delta|tool_call_delta|completed|error",
		"content": "..." // Event-specific data
	},
	"timestamp": "2025-01-15T10:30:00Z"
}
```

#### 4. Control (bidirectional)

```json
{
	"type": "control",
	"id": "C-keepalive-001",
	"data": {
		"command": "ping|pong|close|resume",
		"reason": "optional explanation",
		"payload": null
	},
	"timestamp": "2025-01-15T10:30:00Z"
}
```

#### 5. ACK (bidirectional)

```json
{
	"type": "ack",
	"id": "Q-0123456789abcdef0123456789abcdef",
	"data": {
		"acked_at": "2025-01-15T10:30:00.001Z",
		"sequence": 42
	},
	"timestamp": "2025-01-15T10:30:00Z"
}
```

### Message Delivery Semantics

| Scenario             | Semantics     | Implementation                     |
| -------------------- | ------------- | ---------------------------------- |
| ServerQuery delivery | At-least-once | Resend on timeout + ACK            |
| QueryResponse        | At-most-once  | No ACK needed (idempotent queries) |
| LlmEvent             | At-most-once  | Stream is ephemeral                |
| Control              | Best-effort   | No ACK, fire-and-forget            |

---

## 4. Endpoint Definition

### WebSocket Upgrade Endpoint

```
GET /v1/ws/sessions/{session_id}
  ?protocol_version=3.0
  &features=backpressure,ack,compression

Headers:
  Upgrade: websocket
  Connection: Upgrade
  Sec-WebSocket-Key: x3JJHMbDL1EzLkh9GBhXDw==
  Sec-WebSocket-Version: 13

Response:
  101 Switching Protocols
  Upgrade: websocket
  Connection: Upgrade
  Sec-WebSocket-Accept: HSmrc0sMlYUkAGmm5OPpG2HaGWk=
```

### Query Parameters

| Param              | Type   | Default | Purpose                                   |
| ------------------ | ------ | ------- | ----------------------------------------- |
| `protocol_version` | string | "3.0"   | Backward compat negotiation               |
| `features`         | string | ""      | CSV feature list (compression, ack, etc.) |
| `reconnect_token`  | string | ""      | Resume previous session                   |

### Authentication

- Reuse existing session authentication
- Validate `session_id` from URL path
- Check session ownership/permissions
- Per-session authorization (existing model)

---

## 5. Migration Path

### Phase 3a: Foundation (Weeks 1-2)

**Goals**:

- WebSocket server implementation
- Message serialization/deserialization
- Basic connection lifecycle

**Tasks**:

1. Implement `websocket.rs` module
   - `WebSocketConnection` struct
   - Message enum with all types
   - Connection state machine

2. Add WebSocket dependencies
   - `tokio-tungstenite` or `axum-websockets`
   - JSON serialization (existing `serde_json`)

3. Implement WebSocket endpoint
   - `GET /v1/ws/sessions/{session_id}`
   - Upgrade negotiation
   - Initial handshake

4. Unit tests
   - Message serialization
   - State transitions
   - Error cases

**Testing**: Unit tests only, no load testing yet

---

### Phase 3b: Core Message Flow (Weeks 2-3)

**Goals**:

- Message routing
- ServerQuery delivery
- QueryResponse handling
- LLM event forwarding

**Tasks**:

1. Integrate with `ServerQueryManager`
   - Use existing send/receive logic
   - Replace HTTP transport with WebSocket
   - Preserve backward compat (HTTP fallback)

2. LLM event forwarding
   - Route SSE events → WebSocket messages
   - Preserve event ordering
   - Handle backpressure

3. Connection lifecycle
   - Graceful close
   - Error recovery
   - Reconnection logic

4. Integration tests
   - Query/response roundtrip
   - LLM event streaming
   - Connection failures

**Testing**: Integration tests with mock client

---

### Phase 3c: Production Hardening (Weeks 3-4)

**Goals**:

- Backpressure handling
- Error recovery
- Monitoring/observability

**Tasks**:

1. Backpressure implementation
   - Message queue limiting (1000 messages/connection)
   - Exponential backoff
   - Metrics for queue depth

2. Keepalive / heartbeat
   - Ping/pong frames every 30 seconds
   - Timeout detection
   - Automatic reconnection

3. Metrics & logging
   - Per-connection: bytes sent/recv, messages, latency
   - Global: connections, message rates, errors
   - Structured logging

4. Error handling
   - Malformed messages
   - Network disconnects
   - Server crashes
   - Client divergence

**Testing**: Chaos engineering tests, connection instability

---

### Phase 3d: Load Testing & Optimization (Weeks 4-5)

**Goals**:

- Validate performance targets
- Identify bottlenecks
- Optimize resource usage

**Load Test Scenarios**:

1. **Sustained Load**
   - 1,000 concurrent connections
   - 100 qps per connection (100k qps aggregate)
   - 30 minutes runtime
   - Metrics: p50/p99 latency, error rate, CPU/memory

2. **Burst Handling**
   - 10,000 new connections in 1 second
   - 100 qps from each
   - Metrics: connection establishment time, success rate

3. **Large Messages**
   - 10MB query response
   - Chunked delivery (WebSocket frame fragmentation)
   - Metrics: throughput, memory usage, frame count

4. **Network Instability**
   - 5% packet loss
   - 100-500ms latency jitter
   - Connection drops (1% per second)
   - Metrics: reconnection latency, data integrity

5. **Resource Limits**
   - Scale to 100k connections
   - Identify server capacity
   - Memory/CPU saturation point

**Success Criteria**:

- ✅ p99 latency <100ms (target: <50ms)
- ✅ Error rate <0.1%
- ✅ Memory per connection <100KB
- ✅ CPU stable (no unbounded growth)
- ✅ Reconnection within 5 seconds

---

### Phase 3e: Gradual Rollout (Weeks 5-6)

**Goals**:

- Deploy to production
- Monitor real-world performance
- Migrate clients

**Deployment Strategy**:

1. **Canary (1% traffic)**
   - Monitor error rates, latency
   - 1 day minimum

2. **Gradual ramp (10% → 50% → 100%)**
   - 1% per day
   - Kill switch: feature flag to disable WebSocket
   - Metrics: compare SSE vs WebSocket latency

3. **Client migration**
   - Ship client update with WebSocket support
   - Clients prefer WebSocket if available
   - Fallback to SSE if upgrade fails
   - Monitor adoption curve

4. **HTTP endpoint deprecation** (Phase 3f, later)
   - Keep HTTP endpoints for 6+ months
   - Warn in docs that WebSocket preferred
   - Monitor HTTP endpoint usage
   - Sunset after adoption reaches 95%+

---

## 6. Backward Compatibility Strategy

### HTTP/SSE Fallback

1. **Simultaneous Support**
   - WebSocket endpoint operational in parallel with SSE
   - Client chooses transport (WebSocket preferred)
   - Server routes messages appropriately

2. **Gradual Deprecation**
   - Phase 3 release: Both transports fully functional
   - Phase 4 release: HTTP endpoint marked deprecated (docs)
   - Phase 5 (6+ months): HTTP endpoint sunset

3. **Fallback Logic** (Client-side)
   ```javascript
   try {
     ws = new WebSocket(`wss://server/v1/ws/sessions/${id}`)
   } catch (e) {
     // Fall back to SSE + HTTP polling
     use old transport stack
   }
   ```

### Server-Side Routing

```rust
// Both transports call same business logic
struct ServerQueryManager {
  // Core logic: unchanged
}

// HTTP handler (Phase 3+)
async fn handle_query_response(...) {
  manager.receive_response(response).await
}

// WebSocket handler (Phase 3+)
async fn handle_ws_message(...) {
  match msg.type {
    QueryResponse => manager.receive_response(...).await,
    ...
  }
}
```

---

## 7. Implementation Notes

### Key Considerations

1. **Ordering Guarantees**
   - WebSocket preserves message order (single stream)
   - LLM events must not be reordered
   - Queries must be delivered in order

2. **Resource Limits**
   - Max connections per IP: tune based on load testing
   - Max message queue per connection: 1000 messages
   - Max message size: 10MB (configurable)

3. **Error Handling**
   - Malformed JSON → close connection + log
   - Timeout → send control frame before closing
   - Server overload → backpressure signal

4. **Keepalive**
   - Ping every 30 seconds
   - Pong timeout: 10 seconds
   - No pong → close connection (client will reconnect)

### Code Organization

```
crates/loom-server/src/
├── websocket.rs           # Phase 3 module
│   ├── WebSocketConnection struct
│   ├── Message types
│   ├── Connection state machine
│   └── Handler functions
├── server_query.rs        # Updated with Phase 3 notes
└── api.rs                 # Updated to add /v1/ws endpoint
```

### Testing Strategy

```
Unit Tests:
  - Message serialization roundtrip
  - State machine transitions
  - Connection lifecycle
  
Integration Tests:
  - Query/response flow (mock client)
  - LLM event streaming
  - Error recovery
  
Load Tests:
  - Sustained: 1000 conn, 100 qps each
  - Burst: 10k new connections/sec
  - Large messages: 10MB
  - Network instability: 5% loss, 100-500ms latency
  
Chaos Tests:
  - Random connection drops
  - Malformed messages
  - Server pause/resume
```

---

## 8. Cost/Benefit Analysis

### Benefits

| Aspect                | Improvement                     | Impact                      |
| --------------------- | ------------------------------- | --------------------------- |
| Query latency         | 10-100x faster (500ms → 5-50ms) | Much better UX              |
| Connection overhead   | 1 TCP connection vs 2-3         | ~30% resource savings       |
| Polling overhead      | Eliminated                      | ~20% CPU reduction          |
| Server scalability    | Higher connections/server       | Can handle 10x+ more users  |
| Real-time interaction | Much faster feedback            | Enable interactive features |

### Costs

| Aspect                   | Effort       | Notes             |
| ------------------------ | ------------ | ----------------- |
| WebSocket implementation | 2-3 weeks    | Core logic        |
| Load testing             | 1-2 weeks    | Validation        |
| Monitoring/metrics       | 1 week       | Observability     |
| Client migration         | 2-4 weeks    | Staggered rollout |
| HTTP fallback support    | 1-2 weeks    | Backward compat   |
| **Total**                | **~6 weeks** | 1-2 engineers     |

### ROI

- **Time to implement**: 4-6 weeks
- **Latency improvement**: 10-100x
- **Resource savings**: ~30%
- **User experience**: Significant improvement
- **Maintenance burden**: Slightly higher (2 transport layers during transition)

---

## 9. Success Criteria

### Performance Metrics

- [ ] Query latency: p99 <100ms (target <50ms)
- [ ] Error rate: <0.1% on all operations
- [ ] Memory per connection: <100KB
- [ ] CPU usage: Stable (no unbounded growth)
- [ ] Reconnection latency: <5 seconds

### Reliability Metrics

- [ ] Message delivery: 100% (no lost messages)
- [ ] Connection establishment: >99.9% success
- [ ] Query timeout rate: <0.01%
- [ ] No data corruption observed

### Operational Metrics

- [ ] Canary deployment: 0 incidents
- [ ] Client adoption: 95%+ within 1 month
- [ ] Support tickets: No WebSocket-specific issues
- [ ] Server scaling: 10x more concurrent users

---

## 10. Risks & Mitigation

| Risk                  | Likelihood | Impact   | Mitigation                                     |
| --------------------- | ---------- | -------- | ---------------------------------------------- |
| WebSocket spec issues | Low        | High     | Load test early, use established libraries     |
| Network instability   | Medium     | Medium   | Robust error handling, keepalive, reconnection |
| Client compatibility  | Low        | Medium   | Gradual rollout, feature flag, fallback        |
| Resource exhaustion   | Low        | High     | Backpressure, queue limits, metrics            |
| Data loss             | Very low   | Critical | At-least-once delivery for queries, ACKs       |
| Deployment issues     | Medium     | Medium   | Canary rollout, monitoring, kill switch        |

---

## 11. Future Enhancements (Phase 3+)

1. **Message Compression**
   - WebSocket permessage-deflate
   - Reduces bandwidth by ~40% for text

2. **Selective Message Subscription**
   - Client filters: "subscribe to queries only"
   - Reduces message volume, CPU

3. **Message Batching**
   - Multiple messages in single frame
   - Reduce frame overhead

4. **Connection Pooling**
   - Multiple WebSocket connections per session
   - Better throughput for large operations

5. **Binary Protocol**
   - Replace JSON with binary encoding
   - Faster parsing, smaller payloads
   - Requires client/server coordination

---

## Appendix: Related Documents

- [INTEGRATION_SUMMARY.md](INTEGRATION_SUMMARY.md) - Phase 1-2 work
- [ROADMAP.md](ROADMAP.md) - Overall project phases
- [architecture.md](architecture.md) - System architecture
- [streaming.md](streaming.md) - LLM streaming details
