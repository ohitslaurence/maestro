-- Feature Flags System Migration
-- Adds tables for environments, flags, strategies, kill switches, and SDK keys

-- Environments
CREATE TABLE IF NOT EXISTS flag_environments (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    color TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(org_id, name)
);

CREATE INDEX IF NOT EXISTS idx_flag_environments_org_id ON flag_environments(org_id);

-- Flags
CREATE TABLE IF NOT EXISTS flags (
    id TEXT PRIMARY KEY,
    org_id TEXT REFERENCES organizations(id),  -- NULL = platform flag
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    tags TEXT NOT NULL DEFAULT '[]',  -- JSON array
    maintainer_user_id TEXT REFERENCES users(id),
    variants TEXT NOT NULL,  -- JSON array
    default_variant TEXT NOT NULL,
    exposure_tracking_enabled INTEGER NOT NULL DEFAULT 0,  -- Boolean (0 = false, 1 = true)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT,
    UNIQUE(org_id, key)
);

CREATE INDEX IF NOT EXISTS idx_flags_org_id ON flags(org_id);
CREATE INDEX IF NOT EXISTS idx_flags_key ON flags(key);
CREATE INDEX IF NOT EXISTS idx_flags_archived ON flags(archived_at);

-- Flag prerequisites
CREATE TABLE IF NOT EXISTS flag_prerequisites (
    id TEXT PRIMARY KEY,
    flag_id TEXT NOT NULL REFERENCES flags(id) ON DELETE CASCADE,
    prerequisite_flag_key TEXT NOT NULL,
    required_variant TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_flag_prerequisites_flag_id ON flag_prerequisites(flag_id);

-- Strategies (must be created before flag_configs due to foreign key)
CREATE TABLE IF NOT EXISTS flag_strategies (
    id TEXT PRIMARY KEY,
    org_id TEXT REFERENCES organizations(id),  -- NULL = platform strategy
    name TEXT NOT NULL,
    description TEXT,
    conditions TEXT NOT NULL DEFAULT '[]',  -- JSON array
    percentage INTEGER,
    percentage_key TEXT NOT NULL DEFAULT '"user_id"',  -- JSON
    schedule TEXT,  -- JSON
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_flag_strategies_org_id ON flag_strategies(org_id);

-- Flag configs (per environment)
CREATE TABLE IF NOT EXISTS flag_configs (
    id TEXT PRIMARY KEY,
    flag_id TEXT NOT NULL REFERENCES flags(id) ON DELETE CASCADE,
    environment_id TEXT NOT NULL REFERENCES flag_environments(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 0,
    strategy_id TEXT REFERENCES flag_strategies(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(flag_id, environment_id)
);

CREATE INDEX IF NOT EXISTS idx_flag_configs_flag_id ON flag_configs(flag_id);
CREATE INDEX IF NOT EXISTS idx_flag_configs_environment_id ON flag_configs(environment_id);
CREATE INDEX IF NOT EXISTS idx_flag_configs_strategy_id ON flag_configs(strategy_id);

-- Kill switches
CREATE TABLE IF NOT EXISTS kill_switches (
    id TEXT PRIMARY KEY,
    org_id TEXT REFERENCES organizations(id),  -- NULL = platform
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    linked_flag_keys TEXT NOT NULL DEFAULT '[]',  -- JSON array
    is_active INTEGER NOT NULL DEFAULT 0,
    activated_at TEXT,
    activated_by TEXT REFERENCES users(id),
    activation_reason TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(org_id, key)
);

CREATE INDEX IF NOT EXISTS idx_kill_switches_org_id ON kill_switches(org_id);
CREATE INDEX IF NOT EXISTS idx_kill_switches_is_active ON kill_switches(is_active);

-- SDK keys
CREATE TABLE IF NOT EXISTS sdk_keys (
    id TEXT PRIMARY KEY,
    environment_id TEXT NOT NULL REFERENCES flag_environments(id) ON DELETE CASCADE,
    key_type TEXT NOT NULL,  -- 'client_side', 'server_side'
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_sdk_keys_environment_id ON sdk_keys(environment_id);
CREATE INDEX IF NOT EXISTS idx_sdk_keys_key_hash ON sdk_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_sdk_keys_revoked ON sdk_keys(revoked_at);

-- Exposure logs (for experiment tracking)
CREATE TABLE IF NOT EXISTS exposure_logs (
    id TEXT PRIMARY KEY,
    flag_id TEXT NOT NULL REFERENCES flags(id),
    flag_key TEXT NOT NULL,
    environment_id TEXT NOT NULL REFERENCES flag_environments(id),
    user_id TEXT,
    org_id TEXT,
    variant TEXT NOT NULL,
    reason TEXT NOT NULL,  -- JSON
    context_hash TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_exposure_logs_flag_id ON exposure_logs(flag_id);
CREATE INDEX IF NOT EXISTS idx_exposure_logs_timestamp ON exposure_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_exposure_logs_context_hash ON exposure_logs(context_hash, timestamp);

-- Flag statistics
CREATE TABLE IF NOT EXISTS flag_stats (
    flag_id TEXT PRIMARY KEY REFERENCES flags(id) ON DELETE CASCADE,
    last_evaluated_at TEXT,
    evaluation_count_24h INTEGER NOT NULL DEFAULT 0,
    evaluation_count_7d INTEGER NOT NULL DEFAULT 0,
    evaluation_count_30d INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
