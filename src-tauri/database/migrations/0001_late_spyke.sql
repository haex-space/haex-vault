PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_haex_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`owner_did` text NOT NULL,
	`device_id` text,
	`endpoint_id` text NOT NULL,
	`secret_key` text,
	`name` text NOT NULL,
	`platform` text NOT NULL,
	`avatar` text,
	`avatar_options` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`owner_did`) REFERENCES `haex_identities`(`did`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
INSERT INTO `__new_haex_devices`("id", "owner_did", "device_id", "endpoint_id", "secret_key", "name", "platform", "avatar", "avatar_options", "created_at") SELECT "id", "owner_did", "device_id", "endpoint_id", "secret_key", "name", "platform", "avatar", "avatar_options", "created_at" FROM `haex_devices`;--> statement-breakpoint
DROP TABLE `haex_devices`;--> statement-breakpoint
ALTER TABLE `__new_haex_devices` RENAME TO `haex_devices`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_devices_device_id_unique` ON `haex_devices` (`device_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `haex_devices_endpoint_id_unique` ON `haex_devices` (`endpoint_id`);--> statement-breakpoint
CREATE TABLE `__new_haex_peer_shares` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`device_id` text NOT NULL,
	`endpoint_id` text NOT NULL,
	`name` text NOT NULL,
	`local_path` text NOT NULL,
	`authored_by_did` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`device_id`) REFERENCES `haex_devices`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_peer_shares`("id", "space_id", "device_id", "endpoint_id", "name", "local_path", "authored_by_did", "created_at") SELECT "id", "space_id", "device_id", "endpoint_id", "name", "local_path", "authored_by_did", "created_at" FROM `haex_peer_shares`;--> statement-breakpoint
DROP TABLE `haex_peer_shares`;--> statement-breakpoint
ALTER TABLE `__new_haex_peer_shares` RENAME TO `haex_peer_shares`;--> statement-breakpoint
CREATE TABLE `__new_haex_space_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`identity_id` text,
	`device_id` text NOT NULL,
	`endpoint_id` text NOT NULL,
	`name` text NOT NULL,
	`platform` text NOT NULL,
	`avatar` text,
	`avatar_options` text,
	`relay_url` text,
	`leader_priority` integer DEFAULT 10,
	`authored_by_did` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP),
	FOREIGN KEY (`space_id`) REFERENCES `haex_spaces`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`identity_id`) REFERENCES `haex_identities`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`device_id`) REFERENCES `haex_devices`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_haex_space_devices`("id", "space_id", "identity_id", "device_id", "endpoint_id", "name", "platform", "avatar", "avatar_options", "relay_url", "leader_priority", "authored_by_did", "created_at") SELECT "id", "space_id", "identity_id", "device_id", "endpoint_id", "name", "platform", "avatar", "avatar_options", "relay_url", "leader_priority", "authored_by_did", "created_at" FROM `haex_space_devices`;--> statement-breakpoint
DROP TABLE `haex_space_devices`;--> statement-breakpoint
ALTER TABLE `__new_haex_space_devices` RENAME TO `haex_space_devices`;--> statement-breakpoint
CREATE UNIQUE INDEX `haex_space_devices_space_device_unique` ON `haex_space_devices` (`space_id`,`device_id`);--> statement-breakpoint
-- ---------------------------------------------------------------------------
-- FK-parent guard triggers
-- ---------------------------------------------------------------------------
-- haex_space_devices.device_id has a SQL FK on haex_devices.id, but space-CRDT
-- sync delivers rows authored by foreign vaults whose haex_devices.id values
-- never exist locally. The BEFORE INSERT trigger below auto-creates the
-- missing FK parents (a haex_identities stub for the publisher and a
-- haex_devices stub for the device) so the FK check passes for both local
-- inserts and CRDT-applied rows. Use this same pattern whenever a CRDT-synced
-- table needs to FK-reference a vault-private parent table.
-- Notes:
--   - gen_uuid() is the Rust-side UDF registered in open_encrypted_connection.
--   - INSERT OR IGNORE skips when the unique constraint matches (did for
--     haex_identities, id for haex_devices), so own rows are never clobbered.
--   - secret_key / device_id remain NULL for foreign device stubs; we use
--     that as the "is this my device" signal alongside the source='own'
--     check on the owning identity.
--   - During CRDT apply triggers_enabled='0' suppresses the CRDT triggers,
--     so a stub created in that path does NOT propagate via Personal Sync
--     immediately. Other vaults receive the same haex_space_devices CRDT row
--     independently and recreate the stub locally. Stubs created on a local
--     INSERT path (e.g. peer-storage publish) propagate normally.
CREATE TRIGGER `haex_space_devices_ensure_refs`
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
-- Foreign members can rename / re-avatar their device, which arrives as an
-- UPDATE on haex_space_devices and would not refresh the haex_devices stub.
-- The AFTER UPDATE trigger mirrors the snapshot columns back to the stub,
-- but only when the device row is a foreign stub (secret_key IS NULL) so
-- it can never clobber an own device's metadata.
CREATE TRIGGER `haex_space_devices_propagate_meta`
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
END;--> statement-breakpoint
-- Defensive: haex_peer_shares.device_id has the same FK on haex_devices.id.
-- In the healthy flow the parallel haex_space_devices push creates the stub
-- first, but if a peer_share arrives without a prior space_devices row we
-- still need a parent so the FK passes. The stub created here is intentionally
-- minimal (no platform / no name beyond the DID) — it gets refined the moment
-- the matching haex_space_devices row arrives via the propagate_meta trigger.
CREATE TRIGGER `haex_peer_shares_ensure_refs`
BEFORE INSERT ON `haex_peer_shares`
FOR EACH ROW
WHEN NEW.authored_by_did IS NOT NULL
BEGIN
  INSERT OR IGNORE INTO `haex_identities` (id, did, name, source)
    VALUES (gen_uuid(), NEW.authored_by_did, NEW.authored_by_did, 'space');
  INSERT OR IGNORE INTO `haex_devices`
    (id, owner_did, endpoint_id, name, platform)
    VALUES (
      NEW.device_id,
      NEW.authored_by_did,
      NEW.endpoint_id,
      NEW.authored_by_did,
      'unknown'
    );
END;