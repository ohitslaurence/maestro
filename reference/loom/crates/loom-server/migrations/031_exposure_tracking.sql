-- Exposure Tracking Migration
-- Adds exposure_tracking_enabled field to flags table

-- Add exposure_tracking_enabled column to flags table
ALTER TABLE flags ADD COLUMN exposure_tracking_enabled INTEGER NOT NULL DEFAULT 0;

-- Create index for filtering flags with exposure tracking enabled
CREATE INDEX IF NOT EXISTS idx_flags_exposure_tracking ON flags(exposure_tracking_enabled);
