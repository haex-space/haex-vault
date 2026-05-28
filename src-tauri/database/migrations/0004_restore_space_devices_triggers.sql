-- ---------------------------------------------------------------------------
-- HAND-WRITTEN MIGRATION (do not regenerate with drizzle-kit)
-- ---------------------------------------------------------------------------
-- Drizzle-kit cannot model SQLite triggers. The FK-parent guard triggers on
-- `haex_space_devices` (originally created in 0001_late_spyke.sql) are dropped
-- whenever drizzle issues a table-recreate for column / FK changes — most
-- recently in 0003_redundant_vulture.sql, which set ON DELETE CASCADE on
-- `identity_id`. This file restores them so peer-replicated rows can still
-- auto-create their FK parents.
--
-- Keep the trigger bodies in sync with the original definitions in
-- 0001_late_spyke.sql. The `IF NOT EXISTS` guard makes the migration safe
-- to apply on top of vaults that already received the triggers via an
-- earlier hand-edited version of 0003.
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
    VALUES (
      NEW.device_id,
      NEW.authored_by_did,
      NEW.endpoint_id,
      NEW.name,
      NEW.platform,
      NEW.avatar,
      NEW.avatar_options
    );
END;--> statement-breakpoint

CREATE TRIGGER IF NOT EXISTS `haex_space_devices_propagate_meta`
AFTER UPDATE ON `haex_space_devices`
FOR EACH ROW
WHEN EXISTS (
  SELECT 1 FROM `haex_devices`
  WHERE `haex_devices`.id = NEW.device_id
    AND `haex_devices`.secret_key IS NULL
)
BEGIN
  UPDATE `haex_devices`
    SET endpoint_id = NEW.endpoint_id,
        name = NEW.name,
        platform = NEW.platform,
        avatar = NEW.avatar,
        avatar_options = NEW.avatar_options
    WHERE id = NEW.device_id AND secret_key IS NULL;
END;
