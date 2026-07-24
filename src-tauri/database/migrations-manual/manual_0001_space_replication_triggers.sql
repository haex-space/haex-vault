-- ---------------------------------------------------------------------------
-- MANUAL MIGRATION (hand-written; drizzle-kit cannot model SQLite triggers)
-- ---------------------------------------------------------------------------
-- drizzle-kit has no representation for SQLite TRIGGERs, so these FK-parent
-- guard triggers can never be expressed in the TypeScript schema and would be
-- silently dropped from the regenerated drizzle baseline. They live here, in
-- the dedicated manual-migrations folder, and are applied by the Rust runner
-- AFTER the drizzle baseline (see migrations.rs::load_manual_migrations).
--
-- Purpose: haex_space_devices.device_id carries a SQL FK on haex_devices.id.
-- The AFTER UPDATE trigger mirrors renamed/re-avatared foreign-device metadata
-- back onto the device stub (secret_key IS NULL), so foreign device rows in
-- other spaces stay in sync without ever clobbering an own device's metadata.
--
-- The former `*_ensure_refs` BEFORE INSERT triggers (haex_space_devices,
-- haex_peer_shares) were dropped by drizzle migration 0012_column_sigs.sql
-- together with the `authored_by_did` column they depended on. FK-parent
-- stub creation now happens in Rust before the row is applied (ADR 0002,
-- Phase 1 — see Task G1).
--
-- Notes:
--   - IF NOT EXISTS makes this migration idempotent / safe to re-apply.
--   - CRDT columns (haex_hlc, haex_column_hlcs) are injected at runtime by the
--     Rust CrdtTransformer — do NOT add them here. CREATE TRIGGER statements
--     pass through the transformer unchanged.
-- ---------------------------------------------------------------------------

CREATE TRIGGER IF NOT EXISTS `haex_space_devices_propagate_meta`
AFTER UPDATE ON `haex_space_devices`
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM `haex_devices` WHERE `haex_devices`.id = NEW.device_id AND `haex_devices`.secret_key IS NULL)
BEGIN
  UPDATE `haex_devices`
    SET endpoint_id = NEW.endpoint_id, name = NEW.name, platform = NEW.platform, avatar = NEW.avatar, avatar_options = NEW.avatar_options
    WHERE id = NEW.device_id AND secret_key IS NULL;
END;
