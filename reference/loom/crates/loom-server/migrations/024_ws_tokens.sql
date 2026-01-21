-- WebSocket authentication tokens
-- Short-lived tokens for WebSocket first-message authentication
-- Allows web clients to authenticate WebSocket connections without HttpOnly cookie access

CREATE TABLE IF NOT EXISTS ws_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_ws_tokens_token_hash ON ws_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_ws_tokens_user_id ON ws_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_ws_tokens_expires_at ON ws_tokens(expires_at);
