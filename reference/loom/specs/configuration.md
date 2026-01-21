<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Configuration Design

## Overview

Loom uses a client-server architecture where clients connect to a central Loom server, which handles
all LLM API interactions:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Configuration Flow                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌──────────┐         HTTP          ┌──────────┐         HTTPS             │
│   │  Client  │ ───────────────────▶  │  Server  │ ──────────────────▶ LLM   │
│   │          │                       │          │                     API   │
│   │ - server │                       │ - provider                           │
│   │   _url   │                       │ - api_key                            │
│   │          │                       │ - model                              │
│   └──────────┘                       └──────────┘                           │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key principle:** Clients do NOT have LLM API keys. All LLM requests go through the server.

Configuration uses a layered approach within each component:

```
┌─────────────────────────────────────────────────────────────────┐
│                     CLI Arguments                               │
│                  (highest precedence)                           │
├─────────────────────────────────────────────────────────────────┤
│                 Environment Variables                           │
├─────────────────────────────────────────────────────────────────┤
│                    Default Values                               │
│                  (lowest precedence)                            │
└─────────────────────────────────────────────────────────────────┘
```

---

## Client Configuration

The client connects to a Loom server and sends agent requests. It does not interact directly with
LLM APIs.

### CLI Arguments

Defined in [`crates/loom-cli/src/main.rs`](../crates/loom-cli/src/main.rs) using clap derive macros:

```rust
#[derive(Parser, Debug)]
#[command(name = "loom", version, about, long_about = None)]
struct Args {
	/// URL of the Loom server
	#[arg(long, default_value = "http://localhost:8080", env = "LOOM_SERVER_URL")]
	server_url: String,

	/// LLM provider to use (anthropic or openai)
	#[arg(long, default_value = "anthropic", env = "LOOM_LLM_PROVIDER")]
	provider: LlmProvider,

	/// Workspace directory for file operations
	#[arg(short, long, default_value = ".")]
	workspace: PathBuf,

	/// Log level
	#[arg(short, long, default_value = "info")]
	log_level: LogLevel,

	/// Output logs as JSON
	#[arg(long, default_value = "false")]
	json_logs: bool,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum LlmProvider {
	#[default]
	Anthropic,
	OpenAI,
}
```

### Client Argument Reference

| Argument       | Short | Type                                      | Default                 | Env Var           | Description                |
| -------------- | ----- | ----------------------------------------- | ----------------------- | ----------------- | -------------------------- |
| `--server-url` | -     | `String`                                  | `http://localhost:8080` | `LOOM_SERVER_URL` | URL of the Loom server     |
| `--provider`   | -     | `anthropic \| openai`                     | `anthropic`             | `LOOM_LLM_PROVIDER`   | LLM provider to use        |
| `--workspace`  | `-w`  | `PathBuf`                                 | `.`                     | -                 | Workspace directory        |
| `--log-level`  | `-l`  | `trace \| debug \| info \| warn \| error` | `info`                  | -                 | Logging verbosity          |
| `--json-logs`  | -     | `bool`                                    | `false`                 | -                 | Structured JSON log output |

### Log Level Enum

```rust
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum LogLevel {
	Trace,
	Debug,
	#[default]
	Info,
	Warn,
	Error,
}
```

### Client Environment Variables

| Variable          | Description                                                               |
| ----------------- | ------------------------------------------------------------------------- |
| `LOOM_SERVER_URL` | URL of the Loom server (overridden by `--server-url`)                     |
| `LOOM_LLM_PROVIDER`   | LLM provider to use: `anthropic` or `openai` (overridden by `--provider`) |
| `RUST_LOG`        | tracing filter directive (overrides `--log-level`)                        |

---

## Server Configuration

The Loom server handles all LLM provider interactions. **API keys MUST be set on the server only.
Never expose API keys to clients.**

### Secret Type

All API keys and sensitive configuration values are wrapped in the [`Secret<T>`](secret-system.md)
type from the `loom-secret` crate. This ensures:

- Secrets are **never logged** (Debug/Display always shows `[REDACTED]`)
- Secrets are **never serialized** to plain text
- Secrets are **zeroized from memory** on drop
- Access requires explicit `.expose()` call

### LLM Provider Configuration

The server can have both providers configured simultaneously. Clients choose which provider to use
via the `--provider` flag.

| Variable                        | File Variant | Required      | Default                    | Description            |
| ------------------------------- | ------------ | ------------- | -------------------------- | ---------------------- |
| `LOOM_SERVER_ANTHROPIC_API_KEY` | `..._FILE`   | For Anthropic | -                          | Anthropic API key      |
| `LOOM_SERVER_ANTHROPIC_MODEL`   | -            | No            | `claude-sonnet-4-20250514` | Anthropic model        |
| `LOOM_SERVER_OPENAI_API_KEY`    | `..._FILE`   | For OpenAI    | -                          | OpenAI API key         |
| `LOOM_SERVER_OPENAI_MODEL`      | -            | No            | `gpt-4o`                   | OpenAI model           |
| `LOOM_SERVER_OPENAI_ORG`        | -            | No            | -                          | OpenAI organization ID |

### File-Based Secrets

All API key environment variables support a `_FILE` suffix for loading secrets from files. This is
the recommended approach for production deployments using Docker or Kubernetes secrets.

**Precedence:** `VAR_FILE` takes precedence over `VAR` if both are set.

```bash
# Option 1: Direct environment variable (development)
export LOOM_SERVER_OPENAI_API_KEY="sk-xxx"

# Option 2: File-based (production, recommended)
export LOOM_SERVER_OPENAI_API_KEY_FILE="/run/secrets/openai_key"
```

See [Secret System Specification](secret-system.md) for Docker/Kubernetes examples.

> **Note:** Both `LOOM_SERVER_ANTHROPIC_API_KEY` and `LOOM_SERVER_OPENAI_API_KEY` can be set at the
> same time. The `LlmService` will expose both providers via `has_anthropic()` and `has_openai()`
> methods.

### Server Environment Variables

| Variable           | Required | Default   | Description              |
| ------------------ | -------- | --------- | ------------------------ |
| `LOOM_SERVER_HOST` | No       | `0.0.0.0` | Server bind address      |
| `LOOM_SERVER_PORT` | No       | `8080`    | Server port              |
| `RUST_LOG`         | No       | `info`    | tracing filter directive |

### Logging

The `RUST_LOG` environment variable uses tracing's `EnvFilter` syntax:

```rust
let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(format!("loom={}", tracing::Level::from(log_level))));
```

Examples:

- `RUST_LOG=debug` - All debug logs
- `RUST_LOG=loom=trace,reqwest=warn` - Trace for loom, warn for reqwest
- `RUST_LOG=loom_llm_anthropic=debug` - Debug specific crate

---

## Configuration Structs

### AgentConfig

Core agent behavior configuration in
[`crates/loom-core/src/config.rs`](../crates/loom-core/src/config.rs):

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentConfig {
	pub model_name: String,
	pub max_retries: u32,
	#[serde(with = "humantime_serde")]
	pub tool_timeout: Duration,
	#[serde(with = "humantime_serde")]
	pub llm_timeout: Duration,
	pub max_tokens: u32,
	pub temperature: Option<f32>,
}

impl Default for AgentConfig {
	fn default() -> Self {
		Self {
			model_name: "claude-sonnet-4-20250514".to_string(),
			max_retries: 3,
			tool_timeout: Duration::from_secs(30),
			llm_timeout: Duration::from_secs(120),
			max_tokens: 4096,
			temperature: None,
		}
	}
}
```

| Field          | Type          | Default                    | Description                |
| -------------- | ------------- | -------------------------- | -------------------------- |
| `model_name`   | `String`      | `claude-sonnet-4-20250514` | Default model identifier   |
| `max_retries`  | `u32`         | `3`                        | Maximum retry attempts     |
| `tool_timeout` | `Duration`    | `30s`                      | Tool execution timeout     |
| `llm_timeout`  | `Duration`    | `120s`                     | LLM API request timeout    |
| `max_tokens`   | `u32`         | `4096`                     | Maximum tokens in response |
| `temperature`  | `Option<f32>` | `None`                     | Sampling temperature       |

### ServerConfig

Server configuration (used internally by loom-server):

```rust
#[derive(Debug, Clone)]
pub struct ServerConfig {
	pub host: String,
	pub port: u16,
	pub anthropic_config: Option<AnthropicConfig>, // Set if LOOM_SERVER_ANTHROPIC_API_KEY is present
	pub openai_config: Option<OpenAIConfig>,       // Set if LOOM_SERVER_OPENAI_API_KEY is present
}
```

The server supports having both providers configured simultaneously. Each provider is optional and
enabled by setting its respective API key environment variable.

### AnthropicConfig

Anthropic client configuration in
[`crates/loom-llm-anthropic/src/types.rs`](../crates/loom-llm-anthropic/src/types.rs):

```rust
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
	pub api_key: String,
	pub base_url: String,
	pub model: String,
}
```

| Field      | Type     | Default                     | Description       |
| ---------- | -------- | --------------------------- | ----------------- |
| `api_key`  | `String` | Required                    | Anthropic API key |
| `base_url` | `String` | `https://api.anthropic.com` | API endpoint      |
| `model`    | `String` | `claude-sonnet-4-20250514`  | Model identifier  |

### OpenAIConfig

OpenAI client configuration in
[`crates/loom-llm-openai/src/types.rs`](../crates/loom-llm-openai/src/types.rs):

```rust
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
	pub api_key: String,
	pub base_url: String,
	pub model: String,
	pub organization: Option<String>,
}
```

| Field          | Type             | Default                     | Description            |
| -------------- | ---------------- | --------------------------- | ---------------------- |
| `api_key`      | `String`         | Required                    | OpenAI API key         |
| `base_url`     | `String`         | `https://api.openai.com/v1` | API endpoint           |
| `model`        | `String`         | `gpt-4o`                    | Model identifier       |
| `organization` | `Option<String>` | `None`                      | OpenAI organization ID |

### RetryConfig

HTTP retry behavior in [`crates/loom-http/src/retry.rs`](../crates/loom-http/src/retry.rs):

```rust
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
				StatusCode::BAD_GATEWAY,
				StatusCode::SERVICE_UNAVAILABLE,
				StatusCode::GATEWAY_TIMEOUT,
			],
		}
	}
}
```

| Field                | Type              | Default                 | Description                      |
| -------------------- | ----------------- | ----------------------- | -------------------------------- |
| `max_attempts`       | `u32`             | `3`                     | Maximum retry attempts           |
| `base_delay`         | `Duration`        | `200ms`                 | Initial delay before first retry |
| `max_delay`          | `Duration`        | `5s`                    | Maximum delay cap                |
| `backoff_factor`     | `f64`             | `2.0`                   | Exponential backoff multiplier   |
| `jitter`             | `bool`            | `true`                  | Add randomization to delays      |
| `retryable_statuses` | `Vec<StatusCode>` | 429, 408, 502, 503, 504 | HTTP statuses that trigger retry |

---

## Builder Pattern

Provider configs use fluent builder pattern with `with_*` methods:

### AnthropicConfig Builder

```rust
impl AnthropicConfig {
	pub fn new(api_key: impl Into<String>) -> Self {
		Self {
			api_key: api_key.into(),
			base_url: "https://api.anthropic.com".to_string(),
			model: "claude-sonnet-4-20250514".to_string(),
		}
	}

	pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
		self.base_url = base_url.into();
		self
	}

	pub fn with_model(mut self, model: impl Into<String>) -> Self {
		self.model = model.into();
		self
	}
}
```

### OpenAIConfig Builder

```rust
impl OpenAIConfig {
	pub fn new(api_key: impl Into<String>) -> Self {
		Self {
			api_key: api_key.into(),
			base_url: "https://api.openai.com/v1".to_string(),
			model: "gpt-4o".to_string(),
			organization: None,
		}
	}

	pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
		self.base_url = base_url.into();
		self
	}

	pub fn with_model(mut self, model: impl Into<String>) -> Self {
		self.model = model.into();
		self
	}

	pub fn with_organization(mut self, org: impl Into<String>) -> Self {
		self.organization = Some(org.into());
		self
	}
}
```

### Usage Example (Server-side)

```rust
// Server initializes LlmService with all available providers
let service = LlmService::from_env()?;

// Check which providers are available
if service.has_anthropic() {
    println!("Anthropic provider is available");
}
if service.has_openai() {
    println!("OpenAI provider is available");
}

// Use provider-specific methods based on client request endpoint
// /proxy/anthropic/* routes use:
let response = service.complete_anthropic(request).await?;
let stream = service.complete_streaming_anthropic(request).await?;

// /proxy/openai/* routes use:
let response = service.complete_openai(request).await?;
let stream = service.complete_streaming_openai(request).await?;
```

### Usage Example (Client-side)

```rust
// Client selects provider via CLI flag or environment variable
let provider = args.provider; // from --provider flag

let client: Arc<dyn LlmClient> = match provider {
    LlmProvider::Anthropic => Arc::new(ProxyLlmClient::anthropic(&args.server_url)?),
    LlmProvider::OpenAI => Arc::new(ProxyLlmClient::openai(&args.server_url)?),
};

// Or use explicit constructor
let client = ProxyLlmClient::new(&args.server_url, LlmProvider::Anthropic)?;
```

---

## Design Decisions

### Why clap derive over builder

We use clap's derive macros rather than the builder API because:

1. **Type safety**: Enum variants for log levels are validated at compile time
2. **Documentation**: Doc comments become help text automatically
3. **Maintainability**: Adding new arguments requires minimal boilerplate
4. **Consistency**: Derive pattern matches our config struct style

### Server-Side API Keys

API keys are configured only on the server because:

1. **Security**: Keys never leave the server or appear in client logs/history
2. **Centralization**: Single point of key management and rotation
3. **Access control**: Server can implement rate limiting and usage policies
4. **Auditability**: All LLM requests flow through a single point

### Environment Variable Precedence

The precedence order (CLI > env > default) was chosen because:

1. **Explicit intent**: CLI arguments represent immediate user intent
2. **Session config**: Env vars represent machine/session defaults
3. **Zero config**: Defaults allow running without any configuration

### Sensible Defaults

Default values are chosen for common developer workflows:

| Setting        | Default                 | Rationale                                          |
| -------------- | ----------------------- | -------------------------------------------------- |
| Server URL     | `http://localhost:8080` | Local development is most common                   |
| Provider       | Anthropic               | Claude excels at coding tasks                      |
| Log level      | Info                    | Balance of visibility and noise                    |
| Workspace      | `.`                     | Current directory is most common                   |
| Retry attempts | 3                       | Handles transient failures without excessive delay |
| Jitter         | enabled                 | Prevents thundering herd in concurrent usage       |

---

## Future Extensions

### Config File Support

Add TOML/YAML configuration file support:

```toml
# Client: ~/.config/loom/config.toml
[client]
server_url = "https://loom.example.com"
log_level = "info"

# Server: /etc/loom/server.toml
[server]
host = "0.0.0.0"
port = 8080

[server.llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[agent]
max_retries = 3
tool_timeout = "30s"
llm_timeout = "120s"
max_tokens = 4096
```

Loading precedence would become:

```
CLI > Environment > Config File > Defaults
```

### Per-Workspace Configuration

Support `.loom.toml` in workspace root for project-specific settings:

```toml
# /path/to/project/.loom.toml
max_tokens = 8192

[tools]
enabled = ["read_file", "edit_file", "list_files"]
```

### Profile Support

Named configuration profiles for different use cases:

```toml
[profiles.coding]
model = "claude-sonnet-4-20250514"
temperature = 0.0

[profiles.creative]
model = "claude-3-opus-20240229"
temperature = 0.8
```

Usage:

```bash
loom --profile coding
loom --profile creative
```

### Additional Future Options

| Feature           | Description                                         |
| ----------------- | --------------------------------------------------- |
| `--config`        | Explicit config file path                           |
| `--dry-run`       | Show effective configuration without running        |
| Config validation | `loom config validate` subcommand                   |
| Config generation | `loom config init` to create template               |
| Secret management | Integration with system keyring for server API keys |
