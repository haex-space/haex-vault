//! SQL of migration `0040_device_peer_schema` (spec 059, data-model.md): the version state a
//! device showed in its last handshake with this device, kept next to the presence row it belongs
//! to. All three columns are `NULL` for a device whose state was never seen.
//!
//! `device_presence_no_sync` is device-local, so nothing here is tracked by haex-crdt and
//! [`super::migrations::HOLZI_TRIGGER_VERSION`] stays as it is. The migration still counts in the
//! number of holzi migrations that two devices compare, as every migration does.

/// The migration `0040_device_peer_schema`; registered in
/// [`super::migrations::holzi_migration_source`].
pub const PEER_SCHEMA_0040: &str = r#"ALTER TABLE device_presence_no_sync ADD COLUMN peer_protocol INTEGER;
--> statement-breakpoint
ALTER TABLE device_presence_no_sync ADD COLUMN peer_migration INTEGER;
--> statement-breakpoint
ALTER TABLE device_presence_no_sync ADD COLUMN peer_trigger INTEGER;"#;
