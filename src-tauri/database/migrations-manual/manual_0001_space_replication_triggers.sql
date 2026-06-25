-- ---------------------------------------------------------------------------
-- MANUAL MIGRATION (hand-written; drizzle-kit cannot model SQLite triggers)
-- ---------------------------------------------------------------------------
-- drizzle-kit has no representation for SQLite TRIGGERs, so these FK-parent
-- guard triggers can never be expressed in the TypeScript schema and would be
-- silently dropped from the regenerated drizzle baseline. They live here, in
-- the dedicated manual-migrations folder, and are applied by the Rust runner
-- AFTER the drizzle baseline (see migrations.rs::load_manual_migrations).
--
-- These triggers were previously hand-edited into the drizzle migrations
-- (created in 0001_late_spyke.sql, restored in 0004_restore_space_devices_
-- triggers.sql after drizzle table-rebuilds dropped them). The drizzle
-- rebaseline removed those files, so the triggers now have a permanent home.
--
-- Purpose: haex_space_devices.device_id / haex_peer_shares.device_id carry a
-- SQL FK on haex_devices.id, but space-CRDT sync delivers rows authored by
-- foreign vaults whose haex_devices.id / haex_identities.did never exist
-- locally. The BEFORE INSERT triggers auto-create the missing FK parents (a
-- haex_identities stub for the publisher and a haex_devices stub for the
-- device) so the FK check passes for both local inserts and CRDT-applied rows.
-- The AFTER UPDATE trigger mirrors renamed/re-avatared foreign-device metadata
-- back onto the device stub, but only for foreign stubs (secret_key IS NULL)
-- so it can never clobber an own device's metadata.
--
-- Notes:
--   - gen_uuid() is the Rust-side UDF registered in open_encrypted_connection.
--   - INSERT OR IGNORE skips when the unique constraint matches (did for
--     haex_identities, id for haex_devices) so own rows are never clobbered.
--   - IF NOT EXISTS makes this migration idempotent / safe to re-apply.
--   - CRDT columns (haex_hlc, haex_column_hlcs) are injected at runtime by the
--     Rust CrdtTransformer — do NOT add them here. CREATE TRIGGER statements
--     pass through the transformer unchanged.
-- ---------------------------------------------------------------------------

CREATE TRIGGER IF NOT EXISTS `haex_space_devices_ensure_refs`
BEFORE INSERT ON `haex_space_devices`
FOR EACH ROW
WHEN NEW.authored_by_did IS NOT NULL
BEGIN
  INSERT OR IGNORE INTO `haex_identities` (id, did, name, source)
    VALUES (gen_uuid(), NEW.authored_by_did, NEW.authored_by_did, 'space');
  INSERT OR IGNORE INTO `haex_devices`
    (id, owner_did, endpoint_id, name, platform, avatar, avatar_options)
    VALUES (NEW.device_id, NEW.authored_by_did, NEW.endpoint_id, NEW.name, NEW.platform, NEW.avatar, NEW.avatar_options);
END;
--> statement-breakpoint
CREATE TRIGGER IF NOT EXISTS `haex_space_devices_propagate_meta`
AFTER UPDATE ON `haex_space_devices`
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM `haex_devices` WHERE `haex_devices`.id = NEW.device_id AND `haex_devices`.secret_key IS NULL)
BEGIN
  UPDATE `haex_devices`
    SET endpoint_id = NEW.endpoint_id, name = NEW.name, platform = NEW.platform, avatar = NEW.avatar, avatar_options = NEW.avatar_options
    WHERE id = NEW.device_id AND secret_key IS NULL;
END;
--> statement-breakpoint
CREATE TRIGGER IF NOT EXISTS `haex_peer_shares_ensure_refs`
BEFORE INSERT ON `haex_peer_shares`
FOR EACH ROW
WHEN NEW.authored_by_did IS NOT NULL
BEGIN
  INSERT OR IGNORE INTO `haex_identities` (id, did, name, source)
    VALUES (gen_uuid(), NEW.authored_by_did, NEW.authored_by_did, 'space');
  INSERT OR IGNORE INTO `haex_devices`
    (id, owner_did, endpoint_id, name, platform)
    VALUES (NEW.device_id, NEW.authored_by_did, NEW.endpoint_id, NEW.authored_by_did, 'unknown');
END;
