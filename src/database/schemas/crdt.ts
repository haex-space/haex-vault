import {
  sqliteTable,
  text,
} from 'drizzle-orm/sqlite-core'

// Most CRDT metadata tables removed:
// - haexCrdtChanges: Sync now works by scanning actual tables directly
// - haexCrdtSnapshots: Not used
// - haexCrdtSyncStatus: Replaced by timestamps in haexSyncBackends table

/**
 * CRDT Configuration (WITHOUT CRDT - local-only metadata)
 * Stores HLC node ID and last timestamp for this device
 * Used by the HLC service to maintain consistent logical time
 */
export const haexCrdtConfigs = sqliteTable('haex_crdt_configs', {
  key: text('key').primaryKey(),
  type: text('type').notNull(),
  value: text('value').notNull(),
})
export type InsertHaexCrdtConfigs = typeof haexCrdtConfigs.$inferInsert
export type SelectHaexCrdtConfigs = typeof haexCrdtConfigs.$inferSelect

/**
 * Dirty Tables Tracker (WITHOUT CRDT - local-only metadata)
 * Tracks which tables have unsync'd changes
 * This is a lightweight tracking table that gets populated by triggers
 * and cleared after successful sync to all backends
 */
export const haexCrdtDirtyTables = sqliteTable(
  'haex_crdt_dirty_tables',
  {
    tableName: text('table_name').primaryKey(),
    lastModified: text('last_modified')
      .notNull()
      .$defaultFn(() => new Date().toISOString()),
  },
)
export type InsertHaexCrdtDirtyTables = typeof haexCrdtDirtyTables.$inferInsert
export type SelectHaexCrdtDirtyTables = typeof haexCrdtDirtyTables.$inferSelect
