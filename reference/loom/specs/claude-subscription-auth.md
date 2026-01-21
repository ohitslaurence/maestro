# Claude Subscription Authentication Specification

This document specifies how to implement authentication for Claude Pro/Max subscriptions versus the traditional pay-per-use Anthropic API.

## Overview

Anthropic supports two authentication methods for Claude models:

| Method | Type | Use Case | Billing |
|--------|------|----------|---------|
| **Claude Pro/Max** | OAuth 2.0 + PKCE | Subscription users ($20-$200/month) | Fixed monthly, usage limits reset every 5 hours |
| **API Key** | Static key | Pay-per-use API access | Per-token billing |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Authentication Flow                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────┐     ┌─────────────────────────┐     ┌───────────────┐ │
│  │ User selects │────▶│    OAuth/API Handler    │────▶│  Credential   │ │
│  │ auth method  │     │                         │     │   Storage     │ │
│  └──────────────┘     └─────────────────────────┘     └───────────────┘ │
│         │                        │                           │          │
│         ▼                        ▼                           ▼          │
│  ┌──────────────┐     ┌─────────────────────────┐     ┌───────────────┐ │
│  │ Claude Pro/  │     │   Browser OAuth Flow    │     │   HTTP Client │ │
│  │ Max: OAuth   │     │   (claude.ai)           │     │ Initialization│ │
│  │              │     │                         │     │               │ │
│  │ API Key:     │     │   OR                    │     │ Headers:      │ │
│  │ Direct entry │     │   Console OAuth Flow    │     │ - Bearer token│ │
│  └──────────────┘     │   (console.anthropic)   │     │ - x-api-key   │ │
│                       └─────────────────────────┘     └───────────────┘ │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Credential Storage

### Schema

```rust
use serde::{Deserialize, Serialize};

/// OAuth credentials for Claude Pro/Max subscriptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    /// Discriminant field
    #[serde(rename = "type")]
    pub credential_type: String, // "oauth"
    
    /// Refresh token for obtaining new access tokens
    pub refresh: String,
    
    /// Current access token (short-lived)
    pub access: String,
    
    /// Expiration timestamp in milliseconds since epoch
    pub expires: u64,
}

/// API Key credentials for pay-per-use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCredentials {
    /// Discriminant field
    #[serde(rename = "type")]
    pub credential_type: String, // "api"
    
    /// API key (sk-ant-...)
    pub key: String,
}

/// Union type for credential storage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Credentials {
    #[serde(rename = "oauth")]
    OAuth {
        refresh: String,
        access: String,
        expires: u64,
    },
    #[serde(rename = "api")]
    ApiKey { key: String },
}

/// Storage format - maps provider ID to credentials
pub type CredentialStore = std::collections::HashMap<String, Credentials>;
```

### Example Storage (JSON)

```json
{
  "anthropic": {
    "type": "oauth",
    "refresh": "rt_abc123...",
    "access": "at_xyz789...",
    "expires": 1735500000000
  }
}
```

Or for API key:

```json
{
  "anthropic": {
    "type": "api",
    "key": "sk-ant-api03-..."
  }
}
```

## OAuth Implementation

### Constants

```rust
/// Anthropic's public OAuth client ID for CLI tools
pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// OAuth redirect URI
pub const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";

/// Token endpoint
pub const TOKEN_ENDPOINT: &str = "https://console.anthropic.com/v1/oauth/token";

/// OAuth scopes required
pub const SCOPES: &str = "org:create_api_key user:profile user:inference";
```

### OAuth Endpoints

| Endpoint | Purpose |
|----------|---------|
| `https://claude.ai/oauth/authorize` | Claude Pro/Max authorization |
| `https://console.anthropic.com/oauth/authorize` | Console (API key creation) authorization |
| `https://console.anthropic.com/v1/oauth/token` | Token exchange and refresh |
| `https://console.anthropic.com/oauth/code/callback` | Redirect URI |

### Authorization Mode

```rust
/// Authorization mode determines which OAuth endpoint to use
#[derive(Debug, Clone, Copy)]
pub enum AuthMode {
    /// Use claude.ai for Pro/Max subscription OAuth
    Max,
    /// Use console.anthropic.com for API key creation
    Console,
}

impl AuthMode {
    pub fn authorize_url(&self) -> &'static str {
        match self {
            AuthMode::Max => "https://claude.ai/oauth/authorize",
            AuthMode::Console => "https://console.anthropic.com/oauth/authorize",
        }
    }
}
```

### PKCE Generation

```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

/// PKCE (Proof Key for Code Exchange) values
pub struct Pkce {
    /// The code verifier (random string)
    pub verifier: String,
    /// The code challenge (SHA256 hash of verifier, base64url encoded)
    pub challenge: String,
}

impl Pkce {
    /// Generate a new PKCE pair
    pub fn generate() -> Self {
        // Generate 32 random bytes for the verifier
        let mut verifier_bytes = [0u8; 32];
        getrandom::getrandom(&mut verifier_bytes).expect("Failed to generate random bytes");
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // Create challenge as SHA256(verifier) base64url encoded
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        let challenge = URL_SAFE_NO_PAD.encode(hash);

        Self { verifier, challenge }
    }
}
```

### Authorization URL Construction

```rust
use url::Url;

/// Result of initiating authorization
pub struct AuthorizationRequest {
    /// URL to open in browser
    pub url: String,
    /// PKCE verifier to store for token exchange
    pub verifier: String,
}

/// Build the OAuth authorization URL
pub fn authorize(mode: AuthMode) -> AuthorizationRequest {
    let pkce = Pkce::generate();

    let mut url = Url::parse(mode.authorize_url()).unwrap();
    
    {
        let mut params = url.query_pairs_mut();
        params.append_pair("code", "true");
        params.append_pair("client_id", CLIENT_ID);
        params.append_pair("response_type", "code");
        params.append_pair("redirect_uri", REDIRECT_URI);
        params.append_pair("scope", SCOPES);
        params.append_pair("code_challenge", &pkce.challenge);
        params.append_pair("code_challenge_method", "S256");
        params.append_pair("state", &pkce.verifier);
    }

    AuthorizationRequest {
        url: url.to_string(),
        verifier: pkce.verifier,
    }
}
```

### Token Exchange

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct TokenExchangeRequest {
    code: String,
    state: String,
    grant_type: String,
    client_id: String,
    redirect_uri: String,
    code_verifier: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    token_type: String,
}

/// Result of token exchange
pub enum ExchangeResult {
    Success {
        access: String,
        refresh: String,
        /// Expiration timestamp in milliseconds
        expires: u64,
    },
    Failed,
}

/// Exchange authorization code for tokens
pub async fn exchange_code(code: &str, verifier: &str) -> ExchangeResult {
    let client = Client::new();

    // Code format from Anthropic: "{authorization_code}#{state}"
    let parts: Vec<&str> = code.split('#').collect();
    let (auth_code, state) = match parts.as_slice() {
        [code, state] => (*code, *state),
        [code] => (*code, ""),
        _ => return ExchangeResult::Failed,
    };

    let request = TokenExchangeRequest {
        code: auth_code.to_string(),
        state: state.to_string(),
        grant_type: "authorization_code".to_string(),
        client_id: CLIENT_ID.to_string(),
        redirect_uri: REDIRECT_URI.to_string(),
        code_verifier: verifier.to_string(),
    };

    let response = client
        .post(TOKEN_ENDPOINT)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<TokenResponse>().await {
                Ok(tokens) => {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    
                    ExchangeResult::Success {
                        access: tokens.access_token,
                        refresh: tokens.refresh_token,
                        expires: now_ms + (tokens.expires_in * 1000),
                    }
                }
                Err(_) => ExchangeResult::Failed,
            }
        }
        _ => ExchangeResult::Failed,
    }
}
```

### Token Refresh

```rust
#[derive(Debug, Serialize)]
struct RefreshRequest {
    grant_type: String,
    refresh_token: String,
    client_id: String,
}

/// Refresh an expired access token
pub async fn refresh_token(refresh: &str) -> Result<ExchangeResult, Box<dyn std::error::Error>> {
    let client = Client::new();

    let request = RefreshRequest {
        grant_type: "refresh_token".to_string(),
        refresh_token: refresh.to_string(),
        client_id: CLIENT_ID.to_string(),
    };

    let response = client
        .post(TOKEN_ENDPOINT)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Token refresh failed: {}", response.status()).into());
    }

    let tokens: TokenResponse = response.json().await?;
    
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    Ok(ExchangeResult::Success {
        access: tokens.access_token,
        refresh: tokens.refresh_token,
        expires: now_ms + (tokens.expires_in * 1000),
    })
}
```

## Request Header Injection

### Required Headers by Auth Type

```rust
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

/// Beta features header name
const ANTHROPIC_BETA_HEADER: &str = "anthropic-beta";

/// API key header name
const ANTHROPIC_API_KEY_HEADER: &str = "x-api-key";

/// Required beta flags for OAuth authentication
const OAUTH_BETA_FLAGS: &[&str] = &[
    "oauth-2025-04-20",                    // Required for OAuth
    "claude-code-20250219",                // Claude Code features
    "interleaved-thinking-2025-05-14",     // Extended thinking
    "fine-grained-tool-streaming-2025-05-14",
];

/// Build headers for OAuth (Pro/Max) authentication
pub fn build_oauth_headers(access_token: &str, existing_betas: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();

    // Bearer token authorization
    let auth_value = format!("Bearer {}", access_token);
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value).unwrap());

    // Merge beta flags
    let mut betas: Vec<&str> = OAUTH_BETA_FLAGS.to_vec();
    if let Some(existing) = existing_betas {
        for beta in existing.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if !betas.contains(&beta) {
                betas.push(beta);
            }
        }
    }
    
    headers.insert(
        ANTHROPIC_BETA_HEADER,
        HeaderValue::from_str(&betas.join(",")).unwrap(),
    );

    // Ensure x-api-key is NOT present for OAuth
    // (handled by not inserting it)

    headers
}

/// Build headers for API key authentication
pub fn build_api_key_headers(api_key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    // API key header
    headers.insert(
        ANTHROPIC_API_KEY_HEADER,
        HeaderValue::from_str(api_key).unwrap(),
    );

    // Beta flags (without oauth-specific ones)
    let betas = &[
        "claude-code-20250219",
        "interleaved-thinking-2025-05-14",
        "fine-grained-tool-streaming-2025-05-14",
    ];
    
    headers.insert(
        ANTHROPIC_BETA_HEADER,
        HeaderValue::from_str(&betas.join(",")).unwrap(),
    );

    headers
}
```

### HTTP Client with Auto-Refresh

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages OAuth credentials with automatic refresh
pub struct OAuthClient {
    credentials: Arc<RwLock<OAuthCredentials>>,
    http_client: Client,
    on_credentials_updated: Option<Box<dyn Fn(&OAuthCredentials) + Send + Sync>>,
}

impl OAuthClient {
    pub fn new(credentials: OAuthCredentials) -> Self {
        Self {
            credentials: Arc::new(RwLock::new(credentials)),
            http_client: Client::new(),
            on_credentials_updated: None,
        }
    }

    /// Set callback for when credentials are refreshed
    pub fn on_update<F>(mut self, callback: F) -> Self
    where
        F: Fn(&OAuthCredentials) + Send + Sync + 'static,
    {
        self.on_credentials_updated = Some(Box::new(callback));
        self
    }

    /// Check if token is expired (with 60 second buffer)
    fn is_expired(expires: u64) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        expires < now_ms + 60_000 // 60 second buffer
    }

    /// Get a valid access token, refreshing if necessary
    pub async fn get_access_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let creds = self.credentials.read().await;
        
        if !Self::is_expired(creds.expires) {
            return Ok(creds.access.clone());
        }
        
        drop(creds);
        
        // Need to refresh
        let mut creds = self.credentials.write().await;
        
        // Double-check after acquiring write lock
        if !Self::is_expired(creds.expires) {
            return Ok(creds.access.clone());
        }

        match refresh_token(&creds.refresh).await? {
            ExchangeResult::Success { access, refresh, expires } => {
                creds.access = access.clone();
                creds.refresh = refresh;
                creds.expires = expires;
                
                if let Some(ref callback) = self.on_credentials_updated {
                    callback(&creds);
                }
                
                Ok(access)
            }
            ExchangeResult::Failed => {
                Err("Token refresh failed".into())
            }
        }
    }

    /// Make an authenticated request to the Anthropic API
    pub async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
    ) -> Result<reqwest::RequestBuilder, Box<dyn std::error::Error>> {
        let token = self.get_access_token().await?;
        let headers = build_oauth_headers(&token, None);
        
        Ok(self.http_client.request(method, url).headers(headers))
    }
}
```

## API Key Creation via OAuth

For users who want to create an API key through OAuth (rather than using the subscription):

```rust
#[derive(Debug, Deserialize)]
struct CreateApiKeyResponse {
    raw_key: String,
}

/// Create an API key using OAuth credentials
/// This is used when user selects "Create API Key" option
pub async fn create_api_key(access_token: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = Client::new();

    let response = client
        .post("https://api.anthropic.com/api/oauth/claude_cli/create_api_key")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Failed to create API key: {}", response.status()).into());
    }

    let result: CreateApiKeyResponse = response.json().await?;
    Ok(result.raw_key)
}
```

## Authentication Flow Implementation

```rust
/// Authentication method options presented to user
pub enum AuthMethod {
    /// OAuth with Claude Pro/Max subscription
    ClaudeProMax,
    /// OAuth to create a new API key
    CreateApiKey,
    /// Manual API key entry
    ManualApiKey,
}

/// Result of authentication process
pub enum AuthResult {
    /// OAuth credentials obtained
    OAuth {
        access: String,
        refresh: String,
        expires: u64,
    },
    /// API key obtained
    ApiKey { key: String },
    /// Authentication failed
    Failed { reason: String },
}

/// Complete authentication flow
pub async fn authenticate(method: AuthMethod) -> AuthResult {
    match method {
        AuthMethod::ClaudeProMax => {
            // 1. Generate authorization URL
            let auth_req = authorize(AuthMode::Max);
            
            // 2. Open browser and wait for user to paste code
            println!("Open this URL in your browser:");
            println!("{}", auth_req.url);
            println!("\nPaste the authorization code here:");
            
            let mut code = String::new();
            std::io::stdin().read_line(&mut code).unwrap();
            let code = code.trim();
            
            // 3. Exchange code for tokens
            match exchange_code(code, &auth_req.verifier).await {
                ExchangeResult::Success { access, refresh, expires } => {
                    AuthResult::OAuth { access, refresh, expires }
                }
                ExchangeResult::Failed => {
                    AuthResult::Failed { reason: "Token exchange failed".into() }
                }
            }
        }
        
        AuthMethod::CreateApiKey => {
            // 1. Generate authorization URL (console mode)
            let auth_req = authorize(AuthMode::Console);
            
            // 2. Open browser and wait for user to paste code
            println!("Open this URL in your browser:");
            println!("{}", auth_req.url);
            println!("\nPaste the authorization code here:");
            
            let mut code = String::new();
            std::io::stdin().read_line(&mut code).unwrap();
            let code = code.trim();
            
            // 3. Exchange code for tokens
            match exchange_code(code, &auth_req.verifier).await {
                ExchangeResult::Success { access, .. } => {
                    // 4. Use OAuth token to create API key
                    match create_api_key(&access).await {
                        Ok(key) => AuthResult::ApiKey { key },
                        Err(e) => AuthResult::Failed { reason: e.to_string() },
                    }
                }
                ExchangeResult::Failed => {
                    AuthResult::Failed { reason: "Token exchange failed".into() }
                }
            }
        }
        
        AuthMethod::ManualApiKey => {
            println!("Enter your API key:");
            let mut key = String::new();
            std::io::stdin().read_line(&mut key).unwrap();
            AuthResult::ApiKey { key: key.trim().to_string() }
        }
    }
}
```

## Comparison Summary

| Feature | OAuth (Pro/Max) | API Key |
|---------|-----------------|---------|
| **Authentication** | Browser-based PKCE flow | Direct key entry or Console OAuth |
| **Token Type** | Bearer access token | Static API key |
| **Token Lifetime** | Short-lived, auto-refreshes | Permanent until revoked |
| **Header** | `Authorization: Bearer <token>` | `x-api-key: <key>` |
| **Required Beta** | `oauth-2025-04-20` | None |
| **Cost Display** | $0 (subscription) | Per-token pricing |
| **Billing** | Fixed monthly ($20-$200) | Pay-per-use |
| **Usage Limits** | Resets every 5 hours | None (API quota) |

## Security Considerations

1. **Token Storage**: Credentials should be stored with restricted file permissions (e.g., `0600`)
2. **PKCE**: All OAuth flows must use PKCE (S256) to prevent authorization code interception
3. **Token Refresh**: Access tokens are short-lived; implement automatic refresh before expiration
4. **No Client Secret**: The client ID is public; no client secret is required (public client flow)
5. **Secure Memory**: Consider zeroing sensitive strings in memory after use

## Dependencies (Cargo.toml)

```toml
[dependencies]
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
url = "2.5"
sha2 = "0.10"
base64 = "0.21"
getrandom = "0.2"
tokio = { version = "1", features = ["full"] }
```

## References

- [Claude Pro/Max Plans](https://www.anthropic.com/pricing)
- [Anthropic API Documentation](https://docs.anthropic.com/)
- [OAuth 2.0 PKCE RFC 7636](https://datatracker.ietf.org/doc/html/rfc7636)
