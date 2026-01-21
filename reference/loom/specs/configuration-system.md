<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Configuration System Specification

**Status:** Draft\
**Version:** 1.0\
**Last Updated:** 2024-12-17

---

## 1. Overview

### Purpose

The Loom configuration system provides centralized, layered configuration management that allows
users to customize behavior at multiple levels (system, user, workspace) while maintaining sensible
defaults.

### Goals

- **Type-safety:** All configuration values are strongly typed with compile-time guarantees
- **XDG Compliance:** Follow the XDG Base Directory Specification for file locations
- **Multiple Sources:** Support configuration from files, environment variables, and CLI arguments
- **Clear Precedence:** Well-defined precedence rules for when the same setting is specified in
  multiple places
- **Validation:** Comprehensive validation with clear error messages
- **Extensibility:** Easy to add new configuration options without breaking changes

### Non-Goals

- GUI configuration editor
- Real-time configuration synchronization across machines
- Encrypted configuration storage (secrets should use system keyring)

### Related Specifications

- [Secret System](secret-system.md) - `Secret<T>` type for API keys and sensitive values
- [Configuration Design](configuration.md) - CLI arguments and environment variables

---

## 2. XDG Base Directory Compliance

Loom follows the
[XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
for all file storage locations.

### Directory Mapping

| Purpose       | Default Path           | Environment Override     |
| ------------- | ---------------------- | ------------------------ |
| Configuration | `~/.config/loom/`      | `$XDG_CONFIG_HOME/loom/` |
| Data          | `~/.local/share/loom/` | `$XDG_DATA_HOME/loom/`   |
| Cache         | `~/.cache/loom/`       | `$XDG_CACHE_HOME/loom/`  |
| State         | `~/.local/state/loom/` | `$XDG_STATE_HOME/loom/`  |

### File Locations

| File                 | Path                                             |
| -------------------- | ------------------------------------------------ |
| User config          | `$XDG_CONFIG_HOME/loom/config.toml`              |
| System config        | `/etc/loom/config.toml`                          |
| Workspace config     | `.loom/config.toml` (relative to workspace root) |
| Conversation history | `$XDG_DATA_HOME/loom/history/`                   |
| Provider cache       | `$XDG_CACHE_HOME/loom/providers/`                |
| Runtime state        | `$XDG_STATE_HOME/loom/`                          |
| Log files            | `$XDG_STATE_HOME/loom/logs/`                     |

### Path Resolution Algorithm

```rust
fn resolve_xdg_path(xdg_var: &str, default_suffix: &str) -> PathBuf {
	env::var(xdg_var)
		.map(PathBuf::from)
		.unwrap_or_else(|_| {
			let home = env::var("HOME").expect("HOME must be set");
			PathBuf::from(home).join(default_suffix)
		})
		.join("loom")
}
```

### Auto-Creation of Default Configuration

When Loom starts and no user configuration file exists at `$XDG_CONFIG_HOME/loom/config.toml`, a
default configuration file is automatically created with sensible defaults. This ensures:

- **Zero-configuration startup:** Users can run Loom immediately without manual setup
- **Discoverability:** The generated file serves as documentation of available options
- **Customization starting point:** Users can modify the generated file rather than creating from
  scratch

The auto-creation process:

1. Check if user config file exists
2. If not, create parent directories (`~/.config/loom/`) if needed
3. Write the default configuration template
4. Log the creation at INFO level

**Note:** Existing configuration files are never overwritten. The auto-creation only occurs when no
user config file exists.

---

## 3. Configuration Sources and Precedence

Configuration values are resolved using a layered precedence system. Higher precedence values
override lower precedence values.

### Precedence Table

| Priority    | Source                | Precedence Value | Description                      |
| ----------- | --------------------- | ---------------- | -------------------------------- |
| 1 (Highest) | CLI Arguments         | 60               | Command-line flags and options   |
| 2           | Environment Variables | 50               | `LOOM_*` environment variables   |
| 3           | Workspace Config      | 40               | `.loom/config.toml` in workspace |
| 4           | User Config           | 30               | `~/.config/loom/config.toml`     |
| 5           | System Config         | 20               | `/etc/loom/config.toml`          |
| 6 (Lowest)  | Built-in Defaults     | 10               | Compiled-in default values       |

### Precedence Rules

1. **Scalar values:** Higher precedence completely replaces lower precedence
2. **Tables/Maps:** Deep merge with higher precedence keys taking priority
3. **Arrays:** Higher precedence completely replaces (no merge)
4. **Explicit null/None:** Can be used to "unset" a value from a lower layer

### Example Resolution

```toml
# System config (precedence 20)
[global]
default_provider = "ollama"

[logging]
level = "warn"

# User config (precedence 30)
[global]
default_provider = "anthropic" # Overrides system

# Workspace config (precedence 40)
[logging]
level = "debug" # Overrides both system and user
```

Final resolved values:

- `global.default_provider` = `"anthropic"` (from user)
- `logging.level` = `"debug"` (from workspace)

---

## 4. Configuration File Format (TOML)

### Complete Schema

```toml
#
# Loom Configuration File
# Location: ~/.config/loom/config.toml
#

# =============================================================================
# Global Settings
# =============================================================================

[global]
# The default LLM provider to use when none is specified
# Must match a key in [providers.*]
default_provider = "anthropic"

# Ordered list of model preferences for automatic fallback
# First available model in the list will be used
model_preferences = [
	"claude-sonnet-4-20250514",
	"claude-3-5-sonnet-20241022",
	"gpt-4o",
	"llama3.1:70b",
]

# Override workspace root detection (optional)
# If not set, detected from .git, Cargo.toml, package.json, etc.
# workspace_root = "/path/to/workspace"

# =============================================================================
# Provider Configurations
# =============================================================================

[providers.anthropic]
type = "anthropic"
# API key (prefer environment variable ANTHROPIC_API_KEY)
# api_key = "sk-ant-..."
# Base URL override for proxies or compatible APIs
# base_url = "https://api.anthropic.com"
default_model = "claude-sonnet-4-20250514"
# Maximum tokens for responses
max_tokens = 8192

[providers.openai]
type = "openai"
# api_key = "sk-..."
# base_url = "https://api.openai.com/v1"
default_model = "gpt-4o"
max_tokens = 4096
# Organization ID (optional)
# organization = "org-..."

[providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
default_model = "llama3.1:8b"
# No API key required for local Ollama

[providers.custom_provider]
type = "openai_compatible"
base_url = "https://my-llm-proxy.internal.corp"
api_key = "internal-key"
default_model = "custom-model-v1"

# =============================================================================
# Tool Settings
# =============================================================================

[tools]
# Maximum workspace size to index (in megabytes)
max_workspace_size_mb = 500

# Timeout for shell command execution (in seconds)
command_timeout_secs = 30

# Allow shell command execution (security setting)
allow_shell = true

# Maximum file size to read (in bytes)
max_file_size_bytes = 10485760 # 10 MB

# Patterns to exclude from file operations
exclude_patterns = [
	".git",
	"node_modules",
	"target",
	"__pycache__",
	"*.pyc",
	".env*",
]

[tools.workspace]
# Explicit workspace root (overrides auto-detection)
# root = "/path/to/workspace"

# Allow tool operations outside the workspace directory
allow_outside_workspace = false

# Additional paths allowed for file operations (when allow_outside_workspace = false)
allowed_paths = [
	# "/tmp/loom-*",
	# "~/.config/loom/",
]

# =============================================================================
# Logging Configuration
# =============================================================================

[logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Log file path (optional, logs to file if set)
# Supports strftime format specifiers
# file = "~/.local/state/loom/logs/loom-%Y-%m-%d.log"

# Also log to stderr (in addition to file)
log_to_stderr = false

# Structured logging format: "json" or "pretty"
format = "pretty"

# Include source code location in logs
include_location = false

# Include span information for tracing
include_spans = true

# =============================================================================
# Retry Configuration
# =============================================================================

[retry]
# Maximum number of retry attempts for transient failures
max_retries = 3

# Initial backoff delay (in milliseconds)
initial_backoff_ms = 1000

# Maximum backoff delay (in milliseconds)
max_backoff_ms = 30000

# Backoff multiplier for exponential backoff
backoff_multiplier = 2.0

# Jitter factor (0.0 to 1.0) to randomize backoff
jitter = 0.1

# Retry on these HTTP status codes
retry_status_codes = [429, 500, 502, 503, 504]

# =============================================================================
# TUI Configuration
# =============================================================================

[tui]
# Color theme: "dark", "light", "auto"
theme = "auto"

# Show token usage statistics
show_token_usage = true

# Enable mouse support
mouse_enabled = true

# Scroll speed (lines per scroll event)
scroll_speed = 3

# =============================================================================
# Session Configuration
# =============================================================================

[session]
# Auto-save conversation history
auto_save = true

# Maximum conversations to keep in history
max_history = 100

# Session timeout (in minutes, 0 = never)
timeout_minutes = 0
```

---

## 5. Environment Variable Mapping

### Convention

Environment variables follow the pattern: `LOOM_SERVER_<SECTION>__<FIELD>`

- Sections are separated by double underscores (`__`)
- All uppercase
- Nested keys use double underscores for each level

### Mapping Table

| Configuration Path             | Environment Variable                         |
| ------------------------------ | -------------------------------------------- |
| `global.default_provider`      | `LOOM_SERVER_GLOBAL__DEFAULT_PROVIDER`       |
| `global.workspace_root`        | `LOOM_SERVER_GLOBAL__WORKSPACE_ROOT`         |
| `providers.openai.api_key`     | `LOOM_SERVER_PROVIDERS__OPENAI__API_KEY`     |
| `providers.anthropic.api_key`  | `LOOM_SERVER_PROVIDERS__ANTHROPIC__API_KEY`  |
| `providers.anthropic.base_url` | `LOOM_SERVER_PROVIDERS__ANTHROPIC__BASE_URL` |
| `tools.command_timeout_secs`   | `LOOM_SERVER_TOOLS__COMMAND_TIMEOUT_SECS`    |
| `tools.workspace.root`         | `LOOM_SERVER_TOOLS__WORKSPACE__ROOT`         |
| `logging.level`                | `LOOM_SERVER_LOGGING__LEVEL`                 |
| `logging.file`                 | `LOOM_SERVER_LOGGING__FILE`                  |
| `retry.max_retries`            | `LOOM_SERVER_RETRY__MAX_RETRIES`             |

### Special Environment Variables

These legacy/convenience variables are also supported:

| Variable            | Maps To                       |
| ------------------- | ----------------------------- |
| `ANTHROPIC_API_KEY` | `providers.anthropic.api_key` |
| `OPENAI_API_KEY`    | `providers.openai.api_key`    |
| `OLLAMA_HOST`       | `providers.ollama.base_url`   |
| `LOOM_LOG`          | `logging.level`               |
| `LOOM_WORKSPACE`    | `global.workspace_root`       |

### Type Coercion

Environment variables are strings that get parsed into the appropriate type:

| Target Type        | Parsing Rules                                                     |
| ------------------ | ----------------------------------------------------------------- |
| `String`           | Used as-is                                                        |
| `bool`             | `"true"`, `"1"`, `"yes"` → true; `"false"`, `"0"`, `"no"` → false |
| `u32`, `i32`, etc. | Standard integer parsing                                          |
| `f64`              | Standard float parsing                                            |
| `Vec<String>`      | Comma-separated values: `"a,b,c"` → `["a", "b", "c"]`             |
| `Duration`         | Integer milliseconds or strings like `"30s"`, `"5m"`              |

---

## 6. Configuration Registry Architecture

### Core Traits

```rust
/// A source of configuration values
pub trait ConfigSource: Send + Sync {
	/// Unique identifier for this source
	fn name(&self) -> &str;

	/// Precedence value (higher = more priority)
	fn precedence(&self) -> u32;

	/// Load configuration from this source
	fn load(&self) -> Result<ConfigLayer, ConfigError>;

	/// Whether this source supports watching for changes
	fn supports_watch(&self) -> bool {
		false
	}

	/// Watch for configuration changes
	fn watch(&self) -> Option<Pin<Box<dyn Stream<Item = ConfigLayer> + Send>>> {
		None
	}
}
```

### ConfigRegistry

```rust
/// Central registry for managing configuration sources and resolution
pub struct ConfigRegistry {
	sources: Vec<Box<dyn ConfigSource>>,
	cache: RwLock<Option<LoomConfig>>,
	watchers: Vec<tokio::sync::watch::Sender<LoomConfig>>,
}

impl ConfigRegistry {
	/// Create a new registry with default sources
	fn new() -> Self;

	/// Register a configuration source
	fn register_source(&mut self, source: Box<dyn ConfigSource>);

	/// Load and resolve all configuration layers
	fn load(&self) -> Result<LoomConfig, ConfigError>;

	/// Get a watch channel for configuration changes
	fn watch(&self) -> tokio::sync::watch::Receiver<LoomConfig>;

	/// Force reload of all configuration
	fn reload(&self) -> Result<LoomConfig, ConfigError>;
}
```

### Layer Merging Algorithm

```rust
impl ConfigLayer {
	/// Deep merge two configuration layers
	///
	/// # Rules:
	/// - `other` values take precedence over `self` values
	/// - Tables are merged recursively
	/// - Arrays are replaced entirely (no merge)
	/// - `None` values in `other` preserve `self` values
	/// - Explicit `null` in TOML clears the value
	pub fn merge(&self, other: &ConfigLayer) -> ConfigLayer {
		// For each field in the config:
		// 1. If other.field is Some, use other.field
		// 2. If other.field is None, use self.field
		// 3. For nested structs, recursively merge
		// 4. For HashMaps, merge keys with other taking precedence
	}
}

/// Merge all layers in precedence order
pub fn merge_layers(layers: Vec<(u32, ConfigLayer)>) -> ConfigLayer {
	let mut sorted = layers;
	sorted.sort_by_key(|(precedence, _)| *precedence);

	sorted
		.into_iter()
		.map(|(_, layer)| layer)
		.reduce(|acc, layer| acc.merge(&layer))
		.unwrap_or_default()
}
```

### Finalization Process

```rust
impl ConfigLayer {
	/// Convert a partial configuration layer into a complete runtime
	/// configuration
	///
	/// This process:
	/// 1. Applies built-in defaults for any missing values
	/// 2. Validates all required fields are present
	/// 3. Validates cross-field constraints
	/// 4. Resolves path variables (e.g., ~ expansion)
	/// 5. Returns an immutable, validated configuration
	pub fn finalize(self) -> Result<LoomConfig, ConfigError> {
		let with_defaults = self.apply_defaults();
		with_defaults.validate()?;
		Ok(with_defaults.into_runtime_config())
	}
}
```

---

## 7. Type Definitions

### Runtime Configuration

```rust
/// Complete, validated runtime configuration
/// All fields are required and have valid values
#[derive(Debug, Clone)]
pub struct LoomConfig {
	pub global: GlobalConfig,
	pub providers: HashMap<String, ProviderConfig>,
	pub tools: ToolsConfig,
	pub logging: LoggingConfig,
	pub retry: RetryConfig,
	pub tui: TuiConfig,
	pub session: SessionConfig,
	pub paths: PathsConfig,
}

#[derive(Debug, Clone)]
pub struct GlobalConfig {
	pub default_provider: String,
	pub model_preferences: Vec<String>,
	pub workspace_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ToolsConfig {
	pub max_workspace_size_mb: u32,
	pub command_timeout: Duration,
	pub allow_shell: bool,
	pub max_file_size_bytes: u64,
	pub exclude_patterns: Vec<String>,
	pub workspace: WorkspaceConfig,
}

#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
	pub root: Option<PathBuf>,
	pub allow_outside_workspace: bool,
	pub allowed_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
	pub level: tracing::Level,
	pub file: Option<PathBuf>,
	pub log_to_stderr: bool,
	pub format: LogFormat,
	pub include_location: bool,
	pub include_spans: bool,
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
	pub max_retries: u32,
	pub initial_backoff: Duration,
	pub max_backoff: Duration,
	pub backoff_multiplier: f64,
	pub jitter: f64,
	pub retry_status_codes: Vec<u16>,
}
```

### Configuration Layer (Partial)

```rust
/// Partial configuration for merging
/// All fields are optional to support sparse layers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigLayer {
	pub global: Option<GlobalConfigLayer>,
	pub providers: Option<HashMap<String, ProviderConfigLayer>>,
	pub tools: Option<ToolsConfigLayer>,
	pub logging: Option<LoggingConfigLayer>,
	pub retry: Option<RetryConfigLayer>,
	pub tui: Option<TuiConfigLayer>,
	pub session: Option<SessionConfigLayer>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfigLayer {
	pub default_provider: Option<String>,
	pub model_preferences: Option<Vec<String>>,
	pub workspace_root: Option<PathBuf>,
}

// Similar pattern for other *Layer types...
```

### XDG Paths

```rust
/// Resolved XDG-compliant paths
#[derive(Debug, Clone)]
pub struct PathsConfig {
	/// Configuration directory: $XDG_CONFIG_HOME/loom/
	pub config_dir: PathBuf,

	/// Data directory: $XDG_DATA_HOME/loom/
	pub data_dir: PathBuf,

	/// Cache directory: $XDG_CACHE_HOME/loom/
	pub cache_dir: PathBuf,

	/// State directory: $XDG_STATE_HOME/loom/
	pub state_dir: PathBuf,
}

impl PathsConfig {
	pub fn from_environment() -> Result<Self, ConfigError> {
		Ok(Self {
			config_dir: resolve_xdg_path("XDG_CONFIG_HOME", ".config")?,
			data_dir: resolve_xdg_path("XDG_DATA_HOME", ".local/share")?,
			cache_dir: resolve_xdg_path("XDG_CACHE_HOME", ".cache")?,
			state_dir: resolve_xdg_path("XDG_STATE_HOME", ".local/state")?,
		})
	}

	pub fn user_config_file(&self) -> PathBuf {
		self.config_dir.join("config.toml")
	}

	pub fn history_dir(&self) -> PathBuf {
		self.data_dir.join("history")
	}

	pub fn log_dir(&self) -> PathBuf {
		self.state_dir.join("logs")
	}
}
```

### Provider Configurations

```rust
#[derive(Debug, Clone)]
pub enum ProviderConfig {
	Anthropic(AnthropicConfig),
	OpenAI(OpenAIConfig),
	Ollama(OllamaConfig),
	OpenAICompatible(OpenAICompatibleConfig),
}

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
	pub api_key: Secret<String>,
	pub base_url: Url,
	pub default_model: String,
	pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct OpenAIConfig {
	pub api_key: Secret<String>,
	pub base_url: Url,
	pub default_model: String,
	pub max_tokens: u32,
	pub organization: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OllamaConfig {
	pub base_url: Url,
	pub default_model: String,
}

#[derive(Debug, Clone)]
pub struct OpenAICompatibleConfig {
	pub api_key: Option<Secret<String>>,
	pub base_url: Url,
	pub default_model: String,
	pub max_tokens: Option<u32>,
}
```

---

## 8. Validation Rules

### Field-Level Validations

| Field                         | Validation Rule                                           |
| ----------------------------- | --------------------------------------------------------- |
| `global.default_provider`     | Must be a key in `providers` map                          |
| `providers.*.api_key`         | Must be non-empty for cloud providers (Anthropic, OpenAI) |
| `providers.*.base_url`        | Must be a valid URL                                       |
| `tools.max_workspace_size_mb` | Must be > 0 and <= 10000                                  |
| `tools.command_timeout_secs`  | Must be >= 1 and <= 3600                                  |
| `tools.max_file_size_bytes`   | Must be > 0 and <= 100MB                                  |
| `logging.level`               | Must be one of: trace, debug, info, warn, error           |
| `retry.max_retries`           | Must be >= 0 and <= 10                                    |
| `retry.initial_backoff_ms`    | Must be >= 100 and <= 60000                               |
| `retry.max_backoff_ms`        | Must be >= `initial_backoff_ms`                           |
| `retry.backoff_multiplier`    | Must be >= 1.0 and <= 10.0                                |
| `retry.jitter`                | Must be >= 0.0 and <= 1.0                                 |

### Cross-Field Validations

```rust
impl ConfigLayer {
	fn validate_cross_field(&self) -> Result<(), ConfigError> {
		// 1. default_provider must exist in providers
		if let Some(ref global) = self.global {
			if let Some(ref default) = global.default_provider {
				if let Some(ref providers) = self.providers {
					if !providers.contains_key(default) {
						return Err(ConfigError::InvalidReference {
							field: "global.default_provider",
							value: default.clone(),
							valid_options: providers.keys().cloned().collect(),
						});
					}
				}
			}
		}

		// 2. model_preferences should reference valid models
		// (warning only, not error)

		// 3. allowed_paths should be absolute or valid patterns
		if let Some(ref tools) = self.tools {
			if let Some(ref workspace) = tools.workspace {
				if let Some(ref paths) = workspace.allowed_paths {
					for path in paths {
						validate_path_pattern(path)?;
					}
				}
			}
		}

		Ok(())
	}
}
```

### Validation Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
	#[error("Configuration file not found: {path}")]
	FileNotFound { path: PathBuf },

	#[error("Invalid TOML syntax in {path}: {message}")]
	ParseError { path: PathBuf, message: String },

	#[error("Invalid value for {field}: {message}")]
	InvalidValue { field: String, message: String },

	#[error("{field} references unknown {target_type} '{value}'. Valid options: {valid_options:?}")]
	InvalidReference {
		field: &'static str,
		value: String,
		valid_options: Vec<String>,
	},

	#[error("Missing required field: {field}")]
	MissingRequired { field: String },

	#[error("Environment variable {var} has invalid value: {message}")]
	InvalidEnvVar { var: String, message: String },
}
```

---

## 9. Loading Algorithm

### Step-by-Step Process

```
┌─────────────────────────────────────────────────────────────┐
│                    Configuration Loading                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. Initialize PathsConfig from XDG environment              │
│    - Resolve $XDG_CONFIG_HOME, $XDG_DATA_HOME, etc.         │
│    - Create directories if they don't exist                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Load Built-in Defaults (precedence: 10)                  │
│    - Hardcoded in binary                                     │
│    - Provides fallback for all optional values               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. Load System Config (precedence: 20)                      │
│    - Path: /etc/loom/config.toml                            │
│    - Skip if not found (optional)                            │
│    - Parse TOML into ConfigLayer                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Load User Config (precedence: 30)                        │
│    - Path: $XDG_CONFIG_HOME/loom/config.toml                │
│    - Skip if not found (optional)                            │
│    - Parse TOML into ConfigLayer                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. Detect Workspace Root                                     │
│    - Search upward for .git, Cargo.toml, package.json, etc. │
│    - Use cwd if no markers found                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 6. Load Workspace Config (precedence: 40)                   │
│    - Path: <workspace>/.loom/config.toml                    │
│    - Skip if not found (optional)                            │
│    - Parse TOML into ConfigLayer                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 7. Load Environment Variables (precedence: 50)              │
│    - Scan for LOOM_* variables                               │
│    - Parse into ConfigLayer structure                        │
│    - Handle special legacy variables                         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 8. Load CLI Arguments (precedence: 60)                      │
│    - Parse command-line arguments                            │
│    - Convert to ConfigLayer                                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 9. Merge All Layers                                          │
│    - Sort by precedence (ascending)                          │
│    - Merge sequentially (later overwrites earlier)           │
│    - Deep merge for tables, replace for scalars/arrays       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 10. Validate Configuration                                   │
│     - Check all field constraints                            │
│     - Verify cross-field references                          │
│     - Return errors with context                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 11. Finalize to Runtime Config                              │
│     - Convert ConfigLayer → LoomConfig                       │
│     - Expand path variables (~, env vars)                    │
│     - Parse durations, URLs, etc.                            │
│     - Return immutable LoomConfig                            │
└─────────────────────────────────────────────────────────────┘
```

### Implementation

```rust
pub fn load_config(cli_args: &CliArgs) -> Result<LoomConfig, ConfigError> {
	// Step 1: Initialize paths
	let paths = PathsConfig::from_environment()?;
	ensure_directories_exist(&paths)?;

	// Steps 2-8: Load all sources
	let mut layers: Vec<(u32, ConfigLayer)> = vec![(10, ConfigLayer::defaults())];

	// System config
	if let Some(layer) = load_toml_file("/etc/loom/config.toml")? {
		layers.push((20, layer));
	}

	// User config
	let user_config = paths.config_dir.join("config.toml");
	if let Some(layer) = load_toml_file(&user_config)? {
		layers.push((30, layer));
	}

	// Detect and load workspace config
	let workspace_root = detect_workspace_root()?;
	let workspace_config = workspace_root.join(".loom/config.toml");
	if let Some(layer) = load_toml_file(&workspace_config)? {
		layers.push((40, layer));
	}

	// Environment variables
	layers.push((50, ConfigLayer::from_environment()?));

	// CLI arguments
	layers.push((60, ConfigLayer::from_cli(cli_args)?));

	// Step 9: Merge
	let merged = merge_layers(layers);

	// Step 10-11: Validate and finalize
	let config = merged.finalize()?;

	tracing::debug!(
			providers = ?config.providers.keys().collect::<Vec<_>>(),
			default_provider = %config.global.default_provider,
			"Configuration loaded"
	);

	Ok(config)
}
```

---

## 10. Future Considerations

### Hot-Reloading

- Watch configuration files for changes using `notify` crate
- Debounce rapid changes (100ms window)
- Validate new configuration before applying
- Emit events for components to react to changes
- Some settings may require restart (marked in schema)

```rust
impl ConfigRegistry {
	pub async fn enable_hot_reload(&self) -> Result<(), ConfigError> {
		let (tx, rx) = tokio::sync::watch::channel(self.load()?);

		// Watch all config file paths
		let mut watcher = notify::recommended_watcher(move |event| {
			if let Ok(new_config) = self.load() {
				let _ = tx.send(new_config);
			}
		})?;

		watcher.watch(&self.paths.user_config_file(), RecursiveMode::NonRecursive)?;
		// ... watch other paths

		Ok(())
	}
}
```

### Config Dump Command

```bash
# Show resolved configuration
loom config show

# Show configuration with source annotations
loom config show --with-sources

# Show specific value
loom config get logging.level

# Validate configuration
loom config validate

# Show effective config as TOML
loom config export > config-backup.toml
```

### Remote Configuration Service

- Support fetching configuration from HTTP endpoint
- Useful for team-wide defaults
- Lower precedence than local user config
- Cache with TTL
- Signature verification for security

```toml
[remote_config]
enabled = true
url = "https://config.internal.corp/loom/team-defaults.toml"
refresh_interval_secs = 3600
verify_signature = true
public_key = "..."
```

### Schema Versioning

- Include schema version in config files
- Migration support for breaking changes
- Deprecation warnings for old fields

```toml
# Config file version (for migration support)
config_version = 1

[global]
# ...
```

---

## Appendix A: Complete Default Values

```rust
impl ConfigLayer {
	pub fn defaults() -> Self {
		Self {
			global: Some(GlobalConfigLayer {
				default_provider: Some("anthropic".to_string()),
				model_preferences: Some(vec![
					"claude-sonnet-4-20250514".to_string(),
					"gpt-4o".to_string(),
				]),
				workspace_root: None,
			}),
			providers: Some(HashMap::from([
				(
					"anthropic".to_string(),
					ProviderConfigLayer {
						provider_type: Some(ProviderType::Anthropic),
						base_url: Some("https://api.anthropic.com".to_string()),
						default_model: Some("claude-sonnet-4-20250514".to_string()),
						max_tokens: Some(8192),
						..Default::default()
					},
				),
				(
					"openai".to_string(),
					ProviderConfigLayer {
						provider_type: Some(ProviderType::OpenAI),
						base_url: Some("https://api.openai.com/v1".to_string()),
						default_model: Some("gpt-4o".to_string()),
						max_tokens: Some(4096),
						..Default::default()
					},
				),
				(
					"ollama".to_string(),
					ProviderConfigLayer {
						provider_type: Some(ProviderType::Ollama),
						base_url: Some("http://localhost:11434".to_string()),
						default_model: Some("llama3.1:8b".to_string()),
						..Default::default()
					},
				),
			])),
			tools: Some(ToolsConfigLayer {
				max_workspace_size_mb: Some(500),
				command_timeout_secs: Some(30),
				allow_shell: Some(true),
				max_file_size_bytes: Some(10 * 1024 * 1024),
				exclude_patterns: Some(vec![
					".git".to_string(),
					"node_modules".to_string(),
					"target".to_string(),
				]),
				workspace: Some(WorkspaceConfigLayer {
					allow_outside_workspace: Some(false),
					..Default::default()
				}),
			}),
			logging: Some(LoggingConfigLayer {
				level: Some("info".to_string()),
				log_to_stderr: Some(false),
				format: Some("pretty".to_string()),
				include_location: Some(false),
				include_spans: Some(true),
				..Default::default()
			}),
			retry: Some(RetryConfigLayer {
				max_retries: Some(3),
				initial_backoff_ms: Some(1000),
				max_backoff_ms: Some(30000),
				backoff_multiplier: Some(2.0),
				jitter: Some(0.1),
				retry_status_codes: Some(vec![429, 500, 502, 503, 504]),
			}),
			..Default::default()
		}
	}
}
```

---

## Appendix B: Migration from Previous Versions

_Reserved for future use when configuration schema changes require migration._
