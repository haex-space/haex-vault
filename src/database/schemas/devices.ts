import { sql } from 'drizzle-orm'
import { sqliteTable, text, uniqueIndex } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// Devices — vault-private device registry (NOT shared-space-synced)
//
// Identity model:
// - id           random UUID per vault, opaque PK. Acts as the FK target for
//                haex_space_devices/haex_peer_shares so the stable device-id
//                file value never leaks to peers or to the sync server.
// - deviceId     File-UUID from <app_data>/device_id. Same across all vaults
//                of this user on the same physical device, used to re-discover
//                the row when the vault is opened on this device.
// - endpointId   iroh ed25519 public key — pro (Device × Vault) unterschiedlich,
//                so verschiedene Vaults auf demselben Gerät über die EndpointId
//                nicht korrelierbar sind.
// - secretKey    iroh ed25519 secret key (hex, 32 bytes). Lebt only in der
//                SQLCipher-verschlüsselten Vault-DB, kein Filesystem-Key mehr.
// ---------------------------------------------------------------------------

export const haexDevices = sqliteTable(
  tableNames.haex.devices.name,
  {
    id: text(tableNames.haex.devices.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    deviceId: text(tableNames.haex.devices.columns.deviceId).notNull(),
    endpointId: text(tableNames.haex.devices.columns.endpointId).notNull(),
    secretKey: text(tableNames.haex.devices.columns.secretKey).notNull(),
    name: text(tableNames.haex.devices.columns.name).notNull(),
    platform: text(tableNames.haex.devices.columns.platform).notNull(), // 'desktop' | 'android' | 'ios'
    avatar: text(tableNames.haex.devices.columns.avatar), // Base64 WebP 128x128
    avatarOptions: text(tableNames.haex.devices.columns.avatarOptions), // JSON DiceBear options
    createdAt: text(tableNames.haex.devices.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    uniqueIndex('haex_devices_device_id_unique').on(table.deviceId),
    uniqueIndex('haex_devices_endpoint_id_unique').on(table.endpointId),
  ],
)
export type InsertHaexDevices = typeof haexDevices.$inferInsert
export type SelectHaexDevices = typeof haexDevices.$inferSelect
