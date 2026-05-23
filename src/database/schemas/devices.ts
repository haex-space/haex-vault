import { sql } from 'drizzle-orm'
import { foreignKey, sqliteTable, text, uniqueIndex } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'
import { haexIdentities } from './identity'

// ---------------------------------------------------------------------------
// Devices — vault-private cache of all devices we know about. Acts as the FK
// target for haex_space_devices/haex_peer_shares.
//
// Two row flavours coexist in this table:
//
// - Own devices: deviceId, secretKey set; ownerDid points to a haex_identities
//   row with source='own'. Created by device_create_for_vault / reclaim.
// - Foreign devices (stubs): deviceId/secretKey NULL; ownerDid points to the
//   publisher's identity (source='space'). Created automatically by the
//   `haex_space_devices_ensure_refs` BEFORE INSERT trigger on the first
//   inbound haex_space_devices row from that device. Stubs are required so
//   haex_space_devices.deviceId/haex_peer_shares.deviceId can carry a real
//   SQL FK while remote CRDT rows still satisfy referential integrity.
//
// Identity columns:
// - id           random UUID per vault, opaque PK. Same id for own + foreign
//                rows so haex_space_devices.deviceId resolves uniformly.
// - ownerDid     FK → haex_identities.did. Identifies who owns this device.
// - deviceId     File-UUID from <app_data>/device_id, OWN rows only. Same
//                across all vaults of this user on the same physical device.
// - endpointId   iroh ed25519 public key. Per (device × vault) distinct.
// - secretKey    iroh ed25519 secret key (hex, 32 bytes), OWN rows only.
// ---------------------------------------------------------------------------

export const haexDevices = sqliteTable(
  tableNames.haex.devices.name,
  {
    id: text(tableNames.haex.devices.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    ownerDid: text(tableNames.haex.devices.columns.ownerDid).notNull(),
    deviceId: text(tableNames.haex.devices.columns.deviceId), // NULL for foreign device stubs
    endpointId: text(tableNames.haex.devices.columns.endpointId).notNull(),
    secretKey: text(tableNames.haex.devices.columns.secretKey), // NULL for foreign device stubs
    name: text(tableNames.haex.devices.columns.name).notNull(),
    platform: text(tableNames.haex.devices.columns.platform).notNull(), // 'desktop' | 'android' | 'ios'
    avatar: text(tableNames.haex.devices.columns.avatar), // Base64 WebP 128x128
    avatarOptions: text(tableNames.haex.devices.columns.avatarOptions), // JSON DiceBear options
    createdAt: text(tableNames.haex.devices.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    uniqueIndex('haex_devices_device_id_unique').on(table.deviceId),
    uniqueIndex('haex_devices_endpoint_id_unique').on(table.endpointId),
    foreignKey({
      columns: [table.ownerDid],
      foreignColumns: [haexIdentities.did],
      name: 'haex_devices_owner_did_fk',
    }),
  ],
)
export type InsertHaexDevices = typeof haexDevices.$inferInsert
export type SelectHaexDevices = typeof haexDevices.$inferSelect
