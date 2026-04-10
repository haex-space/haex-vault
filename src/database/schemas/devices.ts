import { sql } from 'drizzle-orm'
import { sqliteTable, text, uniqueIndex } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// Devices — central device registry (CRDT-synced)
// ---------------------------------------------------------------------------

export const haexDevices = sqliteTable(
  tableNames.haex.devices.name,
  {
    id: text(tableNames.haex.devices.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    endpointId: text(tableNames.haex.devices.columns.endpointId).notNull(),
    name: text(tableNames.haex.devices.columns.name).notNull(),
    platform: text(tableNames.haex.devices.columns.platform).notNull(), // 'desktop' | 'android' | 'ios'
    createdAt: text(tableNames.haex.devices.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    uniqueIndex('haex_devices_endpoint_id_unique').on(table.endpointId),
  ],
)
export type InsertHaexDevices = typeof haexDevices.$inferInsert
export type SelectHaexDevices = typeof haexDevices.$inferSelect
