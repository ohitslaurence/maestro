<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Redact System Specification

**Status:** Proposed\
**Version:** 1.0\
**Last Updated:** 2024-12-18

---

## 1. Overview

### Purpose

The `loom-redact` crate provides real-time secret detection and redaction for arbitrary text. It
uses the comprehensive regex patterns from [gitleaks](https://github.com/gitleaks/gitleaks) (200+
patterns covering API keys, tokens, and secrets from major providers) to scan user input, LLM
responses, and logs, replacing detected secrets with `[REDACTED:<rule-id>]` placeholders.

### Goals

- **Comprehensive coverage**: Leverage gitleaks' battle-tested patterns for 200+ secret types
- **High performance**: Keyword pre-filtering and compiled regexes for real-time scanning
- **Low false positives**: Apply entropy thresholds and allowlists from gitleaks
- **Repeatable updates**: Automated process to sync with upstream gitleaks rules
- **Well-tested**: Property-based tests for correctness and idempotence

### Non-Goals

- Scanning git history or commits (use gitleaks directly for that)
- Path-based filtering (not relevant for log/input scanning)
- Hardware-accelerated regex matching

### Relationship to loom-secret

| Crate         | Purpose                                                                                       |
| ------------- | --------------------------------------------------------------------------------------------- |
| `loom-secret` | **Compile-time protection**: Wraps known secrets in `Secret<T>` to prevent accidental logging |
| `loom-redact` | **Runtime detection**: Scans arbitrary text to find and redact unknown/leaked secrets         |

These crates are complementary:

- `loom-secret` prevents secrets you _know about_ from leaking
- `loom-redact` catches secrets you _didn't know about_ or that leaked anyway

---

## 2. Crate Architecture

```
loom-redact/
├── Cargo.toml
├── build.rs                    # TOML → Rust codegen
├── src/
│   ├── lib.rs                  # Public API (Redactor)
│   ├── entropy.rs              # Shannon entropy calculation
│   ├── generated_rules.rs      # include!() from OUT_DIR
│   └── rule.rs                 # RuleSpec and Allowlist types
└── third_party/
    └── gitleaks/
        ├── README.md           # Upstream source and commit hash
        └── gitleaks.toml       # Vendored copy of gitleaks config
```

### Dependency Graph

```
┌─────────────────────────────────────────────────────────────────┐
│                         loom-redact                             │
│  - RuleSpec with compiled Regex                                 │
│  - Redactor with keyword pre-filtering                          │
│  - Entropy + allowlist checks                                   │
│  - Generated from gitleaks.toml at build time                   │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │ uses
                        ┌─────┴─────┐
                        │   regex   │
                        │ once_cell │
                        │   toml    │ (build dependency)
                        └───────────┘
```

---

## 3. Source of Truth: gitleaks.toml

### Why Use gitleaks.toml

The `config/gitleaks.toml` file is the **canonical output** of gitleaks' rule generation:

1. **Already flattened**: Contains all rule metadata (regex, entropy, allowlists, stopwords)
2. **Stable format**: Documented TOML schema with versioning
3. **Battle-tested**: Used by gitleaks in production
4. **Easy to parse**: Standard TOML, no Go AST parsing needed

### TOML Rule Structure

```toml
[[rules]]
id = "anthropic-api-key"
description = "Identified an Anthropic API Key..."
regex = '''\b(sk-ant-api03-[a-zA-Z0-9_\-]{93}AA)(?:[\x60'"\s;]|\\[nr]|$)'''
entropy = 3.5
keywords = ["sk-ant-api03"]

[[rules.allowlists]]
regexes = ['''.+EXAMPLE$''']
stopwords = ["test", "example"]
```

### What We Extract

| Field                      | Usage in loom-redact                                 |
| -------------------------- | ---------------------------------------------------- |
| `id`                       | Rule identifier for `[REDACTED:<id>]` placeholder    |
| `description`              | Documentation only (not used at runtime)             |
| `regex`                    | Detection pattern (compiled to Rust `Regex`)         |
| `entropy`                  | Minimum Shannon entropy threshold                    |
| `secretGroup`              | Which capture group contains the secret (default: 0) |
| `keywords`                 | Fast pre-filter strings (lowercase substring check)  |
| `allowlists[].regexes`     | False positive patterns to skip                      |
| `allowlists[].stopwords`   | Substrings indicating false positives                |
| `allowlists[].regexTarget` | "match" or "line" (we only use "match")              |

### What We Ignore

| Field                  | Reason                                             |
| ---------------------- | -------------------------------------------------- |
| `path`                 | Path-based filtering not relevant for log scanning |
| `allowlists[].commits` | Git commit filtering not applicable                |
| `allowlists[].paths`   | Path filtering not applicable                      |
| `RequiredRules`        | Composite rule dependencies not needed             |
| `SkipReport`           | Reporting control not applicable                   |

---

## 4. Build-Time Code Generation

### Why build.rs (vs. Separate Tool)

- **Automatic**: Cargo runs it on every build
- **Self-contained**: No external tools or Make targets
- **Validated**: Pattern compilation errors fail the build immediately
- **Portable**: Works on all platforms without extra dependencies

### build.rs Flow

```rust
// Pseudocode
fn main() {
	// 1. Read vendored gitleaks.toml
	let toml = read("third_party/gitleaks/gitleaks.toml");
	let config: GitleaksConfig = parse(toml);

	// 2. Generate Rust code
	let mut generated = String::new();
	let mut unsupported = Vec::new();

	for rule in config.rules {
		if let Some(regex_str) = rule.regex {
			// 3. Validate regex compiles in Rust
			match regex::Regex::new(&regex_str) {
				Ok(_) => emit_rule(&mut generated, &rule),
				Err(e) => {
					eprintln!("cargo:warning=Skipping {}: {}", rule.id, e);
					unsupported.push(rule.id);
				}
			}
		}
	}

	// 4. Write to OUT_DIR/generated_rules.rs
	write(out_dir().join("generated_rules.rs"), generated);

	// 5. Rerun if TOML changes
	println!("cargo:rerun-if-changed=third_party/gitleaks/gitleaks.toml");
}
```

### Generated Code Structure

```rust
// OUT_DIR/generated_rules.rs (simplified)
use once_cell::sync::Lazy;
use regex::Regex;

pub static RULES: Lazy<Vec<RuleSpec>> = Lazy::new(|| {
	vec![
			RuleSpec {
					id: "anthropic-api-key",
					regex: Regex::new(r"\b(sk-ant-api03-...)").unwrap(),
					secret_group: 1,
					entropy: Some(3.5),
					keywords: &["sk-ant-api03"],
					allowlists: &[...],
			},
			// ... 200+ more rules
	]
});
```

### Regex Compatibility (Go/RE2 → Rust)

| Feature                     | Go/RE2  | Rust `regex` | Handling      |
| --------------------------- | ------- | ------------ | ------------- |
| Inline flags `(?i)`         | ✓       | ✓            | Compatible    |
| Scoped flags `(?i:...)`     | ✓       | ✓            | Compatible    |
| Negative scoped `(?-i:...)` | ✓       | ✓            | Compatible    |
| POSIX classes `[[:alnum:]]` | ✓       | ✓            | Compatible    |
| Named groups `(?P<name>)`   | ✓       | ✓            | Compatible    |
| Lookahead `(?=...)`         | ✓       | ✗            | **Skip rule** |
| Lookbehind `(?<=...)`       | Partial | ✗            | **Skip rule** |
| Backreferences              | ✗       | ✗            | N/A           |

Rules with unsupported features are:

1. Logged as `cargo:warning`
2. Documented in README
3. Can be manually patched in vendored TOML if critical

---

## 5. Runtime Types

### RuleSpec

```rust
/// A compiled secret detection rule.
pub struct RuleSpec {
	/// Unique rule identifier (e.g., "anthropic-api-key")
	pub id: &'static str,

	/// Compiled detection regex
	pub regex: &'static Regex,

	/// Which capture group contains the secret (0 = whole match)
	pub secret_group: u32,

	/// Minimum Shannon entropy for a match to be considered valid
	pub entropy: Option<f32>,

	/// Lowercase keywords for fast pre-filtering
	pub keywords: &'static [&'static str],

	/// Allowlists for false positive filtering
	pub allowlists: &'static [Allowlist],
}
```

### Allowlist

```rust
/// Filters for reducing false positives.
pub struct Allowlist {
	/// Patterns that indicate a false positive
	pub regexes: &'static [&'static Regex],

	/// Substrings that indicate a false positive
	pub stopwords: &'static [&'static str],
}
```

---

## 6. Redactor API

### Public Interface

```rust
/// A secret redactor using gitleaks-derived patterns.
pub struct Redactor {
	rules: &'static [RuleSpec],
}

impl Redactor {
	/// Create a redactor with all compiled rules.
	fn new() -> Self;

	/// Redact all detected secrets in the input string.
	///
	/// Returns a new string with secrets replaced by `[REDACTED:<rule-id>]`.
	fn redact(&self, input: &str) -> String;

	/// Check if the input contains any detectable secrets.
	fn contains_secret(&self, input: &str) -> bool;
}

impl Default for Redactor {
	fn default() -> Self {
		Self::new()
	}
}

/// Convenience function to redact secrets using the default redactor.
pub fn redact(input: &str) -> String {
	Redactor::default().redact(input)
}
```

### Replacement Format

Secrets are replaced with `[REDACTED:<rule-id>]`:

```
Before: "export ANTHROPIC_API_KEY=sk-ant-api03-abc123..."
After:  "export ANTHROPIC_API_KEY=[REDACTED:anthropic-api-key]"
```

This format:

- Clearly indicates redaction occurred
- Identifies which rule matched (for debugging)
- Does not itself match any secret pattern (idempotence)

---

## 7. Redaction Algorithm

### Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Input Text                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              1. Lowercase for Keyword Pre-filtering              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│           2. For each rule: keyword check → regex scan           │
│              → allowlist filter → entropy check                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│           3. Collect matches, resolve overlaps                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│           4. Build output with redaction placeholders            │
└─────────────────────────────────────────────────────────────────┘
```

### Step 1: Keyword Pre-filtering

Most rules have `keywords` that must be present (case-insensitive) for the rule to apply:

```rust
let lower = input.to_ascii_lowercase();

for rule in &self.rules {
    // Fast path: skip rules whose keywords don't appear
    if !rule.keywords.is_empty() 
        && !rule.keywords.iter().any(|k| lower.contains(k)) {
        continue;
    }
    // ... run regex
}
```

This is the **primary performance optimization**: most rules never run on a given input.

### Step 2: Regex Matching with Capture Groups

```rust
for caps in rule.regex.captures_iter(input) {
    // Extract the secret using the specified capture group
    let secret_match = caps
        .get(rule.secret_group as usize)
        .unwrap_or_else(|| caps.get(0).unwrap());
    
    let range = secret_match.range();
    let secret_str = secret_match.as_str();
    // ...
}
```

### Step 3: Allowlist Filtering

```rust
let is_false_positive = rule.allowlists.iter().any(|al| {
    // Check regex allowlists
    al.regexes.iter().any(|r| r.is_match(secret_str))
    ||
    // Check stopword allowlists
    al.stopwords.iter().any(|sw| secret_str.contains(sw))
});

if is_false_positive {
    continue; // Skip this match
}
```

### Step 4: Entropy Check

```rust
if let Some(min_entropy) = rule.entropy {
    let actual_entropy = shannon_entropy(secret_str);
    if actual_entropy < min_entropy {
        continue; // Skip low-entropy matches (likely false positive)
    }
}
```

### Step 5: Overlap Resolution

When multiple rules match overlapping regions, keep the longest match:

```rust
struct MatchPlan {
    start: usize,
    end: usize,
    rule_id: &'static str,
}

// Sort by start ascending, then by length descending
matches.sort_by_key(|m| (m.start, -(m.end as isize)));

// Keep non-overlapping matches
let mut accepted = Vec::new();
let mut last_end = 0;

for m in matches {
    if m.start >= last_end {
        accepted.push(m);
        last_end = m.end;
    }
}
```

### Step 6: String Reconstruction

```rust
let mut result = String::with_capacity(input.len());
let mut cursor = 0;

for m in &accepted {
    result.push_str(&input[cursor..m.start]);
    result.push_str("[REDACTED:");
    result.push_str(m.rule_id);
    result.push(']');
    cursor = m.end;
}

result.push_str(&input[cursor..]);
result
```

---

## 8. Entropy Calculation

Shannon entropy measures the randomness of a string. Higher entropy indicates more randomness (more
likely to be a real secret).

```rust
/// Calculate Shannon entropy (bits per character) of a string.
pub fn shannon_entropy(s: &str) -> f32 {
	if s.is_empty() {
		return 0.0;
	}

	let mut counts = [0u32; 256];
	let bytes = s.as_bytes();

	for &b in bytes {
		counts[b as usize] += 1;
	}

	let len = bytes.len() as f32;
	let mut entropy = 0.0f32;

	for &count in &counts {
		if count > 0 {
			let p = count as f32 / len;
			entropy -= p * p.log2();
		}
	}

	entropy
}
```

### Typical Entropy Values

| Content Type                | Entropy (bits) |
| --------------------------- | -------------- |
| All same character ("aaaa") | 0.0            |
| English text                | 2.5 - 4.0      |
| Base64 encoded              | 5.0 - 6.0      |
| Random hex                  | 3.5 - 4.0      |
| Random alphanumeric         | 5.0 - 6.0      |
| API keys (high entropy)     | 4.5+           |

---

## 9. Testing Strategy

### Property-Based Tests

Following loom's testing philosophy, property tests verify invariants across random inputs.

#### Property 1: Known Secrets Are Redacted

```rust
proptest! {
		/// **Property: AWS-like access keys are always redacted**
		///
		/// This verifies that generated patterns matching AWS access key
		/// format (AKIA prefix + 16 alphanumeric chars) are detected
		/// and replaced with the appropriate redaction marker.
		#[test]
		fn aws_access_keys_are_redacted(suffix in "[A-Z2-7]{16}") {
				let key = format!("AKIA{}", suffix);
				let input = format!("export AWS_ACCESS_KEY_ID={}", key);
				let output = redact(&input);

				prop_assert!(!output.contains(&key),
						"AWS key should be redacted");
				prop_assert!(output.contains("[REDACTED:aws-access-token]"),
						"Should contain redaction marker");
		}
}
```

#### Property 2: Redaction Is Idempotent

```rust
proptest! {
		/// **Property: Redacting twice produces the same result**
		///
		/// This ensures our redaction markers themselves do not trigger
		/// any detection patterns, preventing infinite expansion or
		/// corruption of already-redacted content.
		#[test]
		fn redaction_is_idempotent(input in ".*") {
				let once = redact(&input);
				let twice = redact(&once);

				prop_assert_eq!(once, twice,
						"Redacting already-redacted content should be a no-op");
		}
}
```

#### Property 3: Non-Secrets Are Mostly Preserved

```rust
proptest! {
		/// **Property: Normal text is not over-redacted**
		///
		/// This is a heuristic check: for random low-entropy text,
		/// we expect few or no redactions. This catches overly broad
		/// patterns that would harm observability.
		#[test]
		fn english_like_text_preserved(input in "[a-zA-Z .,!?]{0,100}") {
				let output = redact(&input);

				// Most normal text should pass through unchanged
				// Allow for rare false positives
				let redaction_count = output.matches("[REDACTED:").count();
				prop_assert!(redaction_count <= 1,
						"Normal text should rarely trigger redaction");
		}
}
```

### Unit Tests for Key Integrations

```rust
#[test]
fn test_anthropic_api_key_redacted() {
	let input = "ANTHROPIC_API_KEY=sk-ant-api03-abc123...";
	let output = redact(&input);

	assert!(!output.contains("sk-ant-api03"));
	assert!(output.contains("[REDACTED:anthropic-api-key]"));
}

#[test]
fn test_openai_api_key_redacted() {
	let input = "OPENAI_API_KEY=sk-proj-abc123...";
	let output = redact(&input);

	assert!(!output.contains("sk-proj-"));
	assert!(output.contains("[REDACTED:"));
}

#[test]
fn test_github_pat_redacted() {
	let input = "gh pat: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
	let output = redact(&input);

	assert!(!output.contains("ghp_"));
	assert!(output.contains("[REDACTED:github-pat]"));
}

#[test]
fn test_aws_credentials_redacted() {
	let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
	let output = redact(&input);

	assert!(!output.contains("AKIAIOSFODNN7EXAMPLE"));
	assert!(output.contains("[REDACTED:aws-access-token]"));
}
```

### Integration Tests

```rust
#[test]
fn test_multiple_secrets_in_log() {
	let log = r#"
        [INFO] Connecting with API key: sk-ant-api03-xxx...
        [DEBUG] GitHub token: ghp_yyyyyy
        [ERROR] AWS credentials: AKIAIOSFODNN7EXAMPLE
    "#;

	let output = redact(log);

	// All secrets should be redacted
	assert!(!output.contains("sk-ant-api03"));
	assert!(!output.contains("ghp_"));
	assert!(!output.contains("AKIA"));

	// Structure should be preserved
	assert!(output.contains("[INFO]"));
	assert!(output.contains("[DEBUG]"));
	assert!(output.contains("[ERROR]"));
}
```

---

## 10. Update Process

### Refreshing from Upstream Gitleaks

When gitleaks releases new patterns:

```bash
# 1. Update local gitleaks clone
cd /home/ghuntley/gitleaks
git pull origin master

# 2. Copy the updated config
cp config/gitleaks.toml /path/to/loom/crates/loom-redact/third_party/gitleaks/

# 3. Update the README with commit hash
cd /path/to/loom/crates/loom-redact/third_party/gitleaks
echo "Source: https://github.com/gitleaks/gitleaks" > README.md
echo "Commit: $(cd /home/ghuntley/gitleaks && git rev-parse HEAD)" >> README.md
echo "Date: $(date -I)" >> README.md

# 4. Build and test
cd /path/to/loom
cargo build -p loom-redact
cargo test -p loom-redact

# 5. Review warnings for unsupported patterns
# (look for cargo:warning in build output)

# 6. Commit changes
git add crates/loom-redact/third_party/gitleaks/
git commit -m "chore(loom-redact): update gitleaks rules to $(date -I)"
```

### Handling Unsupported Patterns

If critical rules are skipped due to regex incompatibility:

1. **Option A**: Accept the gap (document in README)
2. **Option B**: Manually patch the vendored TOML with a Rust-compatible regex
3. **Option C**: Add a custom rule directly in Rust code

---

## 11. Performance Considerations

### Keyword Pre-filtering

The primary optimization. Most rules include keywords that must appear (case-insensitive) for the
pattern to apply:

| Rule                | Keywords                                  |
| ------------------- | ----------------------------------------- |
| `anthropic-api-key` | `["sk-ant-api03"]`                        |
| `github-pat`        | `["ghp_"]`                                |
| `aws-access-token`  | `["akia", "asia", "abia", "acca", "a3t"]` |

For a typical log line, only 1-5 rules will actually run their regex.

### Lazy Compilation

All regexes are compiled once on first use via `once_cell::sync::Lazy`. Subsequent calls reuse the
compiled patterns.

### Benchmarking Guidelines

```rust
#[bench]
fn bench_redact_clean_log(b: &mut Bencher) {
	let log = "Normal log line without any secrets";
	b.iter(|| redact(log));
}

#[bench]
fn bench_redact_with_secret(b: &mut Bencher) {
	let log = "export OPENAI_API_KEY=sk-...";
	b.iter(|| redact(log));
}

#[bench]
fn bench_redact_large_input(b: &mut Bencher) {
	let log = "a".repeat(10_000);
	b.iter(|| redact(&log));
}
```

Target performance: <1ms for typical log lines, <10ms for 10KB inputs.

---

## 12. Future Considerations

### Potential Enhancements

| Feature                     | Description                                |
| --------------------------- | ------------------------------------------ |
| **Configurable rule sets**  | Enable/disable specific rules              |
| **Custom patterns**         | Add loom-specific patterns                 |
| **Aho-Corasick pre-filter** | Single-pass keyword matching for all rules |
| **fancy-regex fallback**    | Support lookahead for critical patterns    |
| **Streaming redaction**     | Process large inputs in chunks             |

### Integration Points

```rust
// In loom-tools (file reading)
pub fn read_file(path: &Path) -> Result<String> {
	let content = fs::read_to_string(path)?;
	Ok(loom_redact::redact(&content))
}

// In loom-llm-proxy (response streaming)
pub fn process_chunk(chunk: &str) -> String {
	loom_redact::redact(chunk)
}

// In tracing subscriber (log output)
impl<S: Subscriber> Layer<S> for RedactingLayer {
	fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
		// Redact event message before output
	}
}
```

---

## Appendix A: Supported Secret Types

The following categories of secrets are detected (partial list):

| Category            | Examples                                           |
| ------------------- | -------------------------------------------------- |
| **AI/LLM**          | Anthropic, OpenAI, Cohere, HuggingFace, Perplexity |
| **Cloud Providers** | AWS, GCP, Azure, DigitalOcean, Heroku              |
| **Version Control** | GitHub, GitLab, Bitbucket                          |
| **Communication**   | Slack, Discord, Twilio, Telegram, SendGrid         |
| **Payment**         | Stripe, Square, Coinbase, PayPal                   |
| **Infrastructure**  | Kubernetes, Docker, Vault, Terraform               |
| **Monitoring**      | Datadog, Sentry, New Relic, Grafana                |
| **Generic**         | Private keys, JWTs, API keys, passwords            |

Full list: See `third_party/gitleaks/gitleaks.toml`

---

## Appendix B: Regex Dialect Differences

### Patterns That May Need Patching

| Pattern Feature           | Example        | Fix                      |
| ------------------------- | -------------- | ------------------------ |
| Positive lookahead `(?=)` | `(?=.*[A-Z])`  | Remove or rewrite        |
| Negative lookahead `(?!)` | `(?!test)`     | Remove or use allowlist  |
| Lookbehind `(?<=)`        | `(?<=Bearer )` | Remove prefix from match |

### Known Incompatible Patterns

(To be populated during build.rs implementation)

---

## Appendix C: Entropy Thresholds by Rule

| Rule Category         | Typical Entropy | Threshold |
| --------------------- | --------------- | --------- |
| AWS access keys       | 3.5 - 4.0       | 3.0       |
| Generic API keys      | 3.5 - 4.5       | 3.5       |
| Private keys (Base64) | 5.0 - 6.0       | 4.5       |
| JWTs                  | 4.5 - 5.5       | 4.0       |

Entropy thresholds are inherited from gitleaks rules.
