<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Secret System Specification

**Status:** Implemented\
**Version:** 1.0\
**Last Updated:** 2024-12-18

---

## 1. Overview

### Purpose

The Loom secret system provides a type-safe wrapper for sensitive values that prevents accidental
exposure through logging, serialization, or debugging. It ensures that API keys, passwords, tokens,
and other secrets are never leaked in logs, error messages, or configuration dumps.

### Goals

- **Type-level protection**: Secrets are wrapped in a type that enforces redaction at compile time
- **Logging safety**: Secrets never appear in structured or unstructured logs
- **Serialization safety**: Secrets serialize as `[REDACTED]` to prevent leaks in config dumps
- **Memory safety**: Secrets are zeroized from memory on drop
- **Explicit access**: Developers must call `.expose()` to access the underlying value
- **File-based loading**: Support for Docker/Kubernetes secret mounting via `*_FILE` convention

### Non-Goals

- Hardware security module (HSM) integration
- Encrypted at-rest storage (use system keyring for that)
- Constant-time comparison (not required for current use cases)

---

## 2. Crate Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         loom-secret                             │
│  - Secret<T> type                                               │
│  - SecretString type alias                                      │
│  - Redacted Debug/Display/Serialize                             │
│  - Zeroize on drop                                              │
│  - No config/env knowledge                                      │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │ depends on
┌─────────────────────────────┴───────────────────────────────────┐
│                      loom-common-config                         │
│  - Re-exports Secret, SecretString, REDACTED                    │
│  - load_secret_env() for VAR/VAR_FILE loading                   │
│  - require_secret_env() for required secrets                    │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
   loom-config        loom-llm-service      loom-github-app
   (layer types)      (API key config)      (private key, webhook)
```

### Rationale

- **`loom-secret`** is a standalone primitive crate with no business logic
- **`loom-common-config`** adds environment loading on top
- Domain crates can depend on `loom-secret` directly without pulling in config logic

---

## 3. Secret<T> Type

### Definition

```rust
use zeroize::Zeroize;

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Secret<T>
where
	T: Zeroize,
{
	inner: T,
}

pub type SecretString = Secret<String>;

pub const REDACTED: &str = "[REDACTED]";
```

### Key Properties

| Property               | Implementation                                     |
| ---------------------- | -------------------------------------------------- |
| **No Deref**           | Must call `.expose()` to access inner value        |
| **Zeroize on drop**    | Memory is zeroed when secret is dropped            |
| **Redacted Debug**     | `format!("{:?}", secret)` → `Secret("[REDACTED]")` |
| **Redacted Display**   | `format!("{}", secret)` → `[REDACTED]`             |
| **Redacted Serialize** | JSON/TOML output: `"[REDACTED]"`                   |
| **Normal Deserialize** | Loads value normally from config files             |
| **Clone**              | Clones the inner value (requires `T: Clone`)       |
| **PartialEq/Eq**       | Compares inner values (requires `T: PartialEq/Eq`) |

### API

```rust
impl<T: Zeroize> Secret<T> {
	/// Create a new secret wrapper
	fn new(inner: T) -> Self;

	/// Explicitly access the inner value (makes access visible in code review)
	fn expose(&self) -> &T;

	/// Mutable access to the inner value
	fn expose_mut(&mut self) -> &mut T;

	/// Consume and return the inner value (clones to maintain zeroization)
	fn into_inner(self) -> T
	where
		T: Clone;
}
```

---

## 4. Structured Logging Integration

### How It Works

The `Secret<T>` type integrates with `tracing` through its `Debug` and `Display` implementations:

```rust
use tracing::info;
use loom_secret::Secret;

let api_key = Secret::new("sk-secret-key".to_string());

// Display format (%): logs "[REDACTED]"
info!(api_key = %api_key, "Configured API");

// Debug format (?): logs "Secret(\"[REDACTED]\")"
info!(?api_key, "Debug logging");

// Both are safe - the actual key never appears in logs
```

### JSON Logging

When using JSON log output (e.g., `tracing-subscriber` with JSON formatter), secrets are still safe:

```json
{
	"timestamp": "2024-12-18T10:00:00Z",
	"level": "INFO",
	"message": "Configured API",
	"api_key": "[REDACTED]"
}
```

### Best Practices

```rust
// ✅ GOOD: Log presence, not value
info!(
	anthropic_configured = api_key.is_some(),
	openai_configured = openai_key.is_some(),
	"Loaded LLM configuration"
);

// ✅ GOOD: Use Display format
info!(api_key = %secret, "Using API key");

// ✅ GOOD: Use Debug format
debug!(?config, "Full configuration");

// ❌ BAD: Never expose in logs
info!(api_key = %secret.expose(), "This leaks the secret!");
```

---

## 5. File-Based Secret Loading

### Convention

The `load_secret_env()` function supports the `*_FILE` convention used by:

- Docker Secrets
- Kubernetes Secrets (mounted as files)
- HashiCorp Vault Agent
- Other secret management systems

### Precedence

1. If `{VAR}_FILE` is set → read secret from that file path
2. Else if `{VAR}` is set → use its value directly
3. Else → return `None`

### Example

```bash
# Option 1: Direct environment variable
export OPENAI_API_KEY="sk-xxx"

# Option 2: File-based (takes precedence if both set)
export OPENAI_API_KEY_FILE="/run/secrets/openai_key"
```

```rust
use loom_common_config::load_secret_env;

// Automatically checks OPENAI_API_KEY_FILE first, then OPENAI_API_KEY
let api_key = load_secret_env("OPENAI_API_KEY")?;

if let Some(key) = api_key {
    println!("Key configured: {}", key); // prints "[REDACTED]"
}
```

### File Format

- A single trailing newline is stripped (common in secret files)
- All other content is preserved as-is
- Empty files result in empty secrets (may fail validation)

### Error Handling

```rust
#[derive(Debug, Error)]
pub enum SecretEnvError {
	#[error("failed to read secret file at {path}: {source}")]
	Io {
		path: PathBuf,
		source: std::io::Error,
	},

	#[error("secret file path in {var} is empty")]
	EmptyPath { var: String },
}
```

---

## 6. Usage in Configuration

### LlmServiceConfig

```rust
pub struct LlmServiceConfig {
	pub provider: LlmProvider,
	pub anthropic_api_key: Option<SecretString>,
	pub openai_api_key: Option<SecretString>,
	// ...
}

impl LlmServiceConfig {
	pub fn from_env() -> Result<Self, ConfigError> {
		let anthropic_api_key = load_secret_env("LOOM_SERVER_ANTHROPIC_API_KEY")?;
		let openai_api_key = load_secret_env("LOOM_SERVER_OPENAI_API_KEY")?;
		// ...
	}
}
```

### GithubAppConfig

```rust
pub struct GithubAppConfig {
	app_id: u64,
	private_key_pem: SecretString, // Always required, always secret
	webhook_secret: Option<SecretString>, /* Optional, but secret if present
	                                * ... */
}
```

### Using Secrets

```rust
// When you need the actual value (e.g., to make an API call)
if let Some(ref key) = config.openai_api_key {
    let client = OpenAIClient::new(key.expose().clone());
}

// The .expose() call is explicit and visible in code review
```

---

## 7. Testing

### Property-Based Tests

The secret system includes property-based tests to verify that secrets never leak:

```rust
proptest! {
		#[test]
		fn debug_never_contains_secret(inner in "[a-zA-Z0-9]{3,50}") {
				let secret = Secret::new(inner.clone());
				let debug_output = format!("{:?}", secret);
				prop_assert!(!debug_output.contains(&inner));
		}

		#[test]
		fn serialize_never_contains_secret(inner in "[a-zA-Z0-9]{3,50}") {
				let secret = Secret::new(inner.clone());
				let json = serde_json::to_string(&secret).unwrap();
				prop_assert!(!json.contains(&inner));
		}
}
```

### Unit Tests

```rust
#[test]
fn test_debug_redacts_api_keys() {
	let config = LlmServiceConfig::new(LlmProvider::OpenAi).with_openai_api_key("sk-super-secret");

	let debug_output = format!("{:?}", config);

	assert!(!debug_output.contains("sk-super-secret"));
	assert!(debug_output.contains("[REDACTED]"));
}
```

---

## 8. Security Considerations

### What This Protects Against

| Threat                          | Protection                      |
| ------------------------------- | ------------------------------- |
| Secrets in application logs     | Debug/Display always redacted   |
| Secrets in error messages       | Debug impl is redacted          |
| Secrets in config dumps         | Serialize always redacted       |
| Secrets in core dumps           | Zeroize on drop clears memory   |
| Accidental string interpolation | No Deref, must call `.expose()` |

### What This Does NOT Protect Against

| Threat                         | Mitigation                        |
| ------------------------------ | --------------------------------- |
| Deliberate `.expose()` in logs | Code review, linting              |
| Memory inspection before drop  | Use shorter-lived secrets         |
| Side-channel attacks           | Out of scope for this system      |
| Secrets in version control     | Use `.gitignore`, secret scanning |

### Best Practices

1. **Minimize exposure scope**: Call `.expose()` as close to usage as possible
2. **Never log exposed values**: `info!(key = %secret.expose())` is always wrong
3. **Use file-based secrets in production**: Avoid environment variables in production
4. **Rotate secrets regularly**: The system makes rotation easier, not unnecessary

---

## 9. Future Considerations

### Potential Enhancements

| Feature                 | Description                                      |
| ----------------------- | ------------------------------------------------ |
| **Keyring integration** | Load secrets from system keyring                 |
| **Vault integration**   | Fetch secrets from HashiCorp Vault               |
| **AWS Secrets Manager** | Fetch secrets from AWS                           |
| **Secret rotation**     | Automatic secret refresh                         |
| **Audit logging**       | Log when secrets are accessed (not their values) |

### Migration Path

If we later need to adopt an external crate like `secrecy`:

```rust
// loom-secret could become a thin wrapper
pub struct Secret<T>(secrecy::Secret<T>);
```

This would be a non-breaking change for consumers.

---

## Appendix A: Environment Variables

### Supported Variables

| Variable                         | File Variant | Used By                |
| -------------------------------- | ------------ | ---------------------- |
| `LOOM_SERVER_ANTHROPIC_API_KEY`        | `..._FILE`   | loom-llm-service       |
| `LOOM_SERVER_OPENAI_API_KEY`           | `..._FILE`   | loom-llm-service       |
| `LOOM_SERVER_GITHUB_APP_PRIVATE_KEY`   | `..._FILE`   | loom-github-app        |
| `LOOM_SERVER_GITHUB_APP_WEBHOOK_SECRET`| `..._FILE`   | loom-github-app        |
| `LOOM_ANTHROPIC_API_KEY`         | `..._FILE`   | loom-config            |
| `LOOM_OPENAI_API_KEY`            | `..._FILE`   | loom-config            |
| `ANTHROPIC_API_KEY`              | `..._FILE`   | loom-config (fallback) |
| `OPENAI_API_KEY`                 | `..._FILE`   | loom-config (fallback) |

---

## Appendix B: Docker/Kubernetes Examples

### Docker Compose

```yaml
services:
  loom-server:
    image: loom-server:latest
    secrets:
      - openai_key
      - anthropic_key
    environment:
      - LOOM_SERVER_OPENAI_API_KEY_FILE=/run/secrets/openai_key
      - LOOM_SERVER_ANTHROPIC_API_KEY_FILE=/run/secrets/anthropic_key

secrets:
  openai_key:
    file: ./secrets/openai.txt
  anthropic_key:
    file: ./secrets/anthropic.txt
```

### Kubernetes

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: loom-server
spec:
  containers:
    - name: loom-server
      image: loom-server:latest
      env:
        - name: LOOM_SERVER_OPENAI_API_KEY_FILE
          value: /etc/secrets/openai-key
      volumeMounts:
        - name: secrets
          mountPath: /etc/secrets
          readOnly: true
  volumes:
    - name: secrets
      secret:
        secretName: loom-api-keys
```
