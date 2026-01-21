-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Add enrichment columns to audit_logs table for SIEM integration

ALTER TABLE audit_logs ADD COLUMN severity TEXT DEFAULT 'info';
ALTER TABLE audit_logs ADD COLUMN trace_id TEXT;
ALTER TABLE audit_logs ADD COLUMN span_id TEXT;
ALTER TABLE audit_logs ADD COLUMN request_id TEXT;
ALTER TABLE audit_logs ADD COLUMN session_context TEXT;
ALTER TABLE audit_logs ADD COLUMN org_context TEXT;

-- Index for severity filtering
CREATE INDEX IF NOT EXISTS idx_audit_logs_severity ON audit_logs(severity);

-- Index for trace correlation
CREATE INDEX IF NOT EXISTS idx_audit_logs_trace_id ON audit_logs(trace_id);
