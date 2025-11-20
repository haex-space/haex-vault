import {
  integer,
  sqliteTable,
  text,
  index,
  primaryKey,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

/**
 * CRDT Changes Table - Value-less logging for efficient sync
 * Only stores metadata about changes, values are read at sync time
 * Uses composite primary key to ensure only one entry per (table, row, column)
 */
export const haexCrdtChanges = sqliteTable(
  tableNames.haex.crdt.changes.name,
  {
    tableName: text(tableNames.haex.crdt.changes.columns.tableName).notNull(),
    rowPks: text(tableNames.haex.crdt.changes.columns.rowPks, {
      mode: 'json',
    }).notNull(),
    columnName: text(tableNames.haex.crdt.changes.columns.columnName), // NULL for DELETE
    operation: text(tableNames.haex.crdt.changes.columns.operation, {
      enum: ['INSERT', 'UPDATE', 'DELETE'],
    }).notNull(),
    hlcTimestamp: text(
      tableNames.haex.crdt.changes.columns.hlcTimestamp,
    ).notNull(),
    syncState: text(tableNames.haex.crdt.changes.columns.syncState, {
      enum: ['pending_upload', 'pending_apply', 'applied'],
    })
      .notNull()
      .default('pending_upload'),
    deviceId: text(tableNames.haex.crdt.changes.columns.deviceId),
    createdAt: text(tableNames.haex.crdt.changes.columns.createdAt)
      .notNull()
      .$defaultFn(() => new Date().toISOString()),
  },
  (table) => [
    primaryKey({
      columns: [table.tableName, table.rowPks, table.columnName],
    }),
    index('idx_crdt_changes_sync_state').on(table.syncState),
    index('idx_crdt_changes_hlc').on(table.hlcTimestamp),
    index('idx_crdt_changes_table_row').on(table.tableName, table.rowPks),
    index('idx_crdt_changes_device_id').on(table.deviceId),
  ],
)
export type InsertHaexCrdtChanges = typeof haexCrdtChanges.$inferInsert
export type SelectHaexCrdtChanges = typeof haexCrdtChanges.$inferSelect

export const haexCrdtSnapshots = sqliteTable(
  tableNames.haex.crdt.snapshots.name,
  {
    snapshotId: text(tableNames.haex.crdt.snapshots.columns.snapshotId)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    created: text(),
    epochHlc: text(tableNames.haex.crdt.snapshots.columns.epochHlc),
    locationUrl: text(tableNames.haex.crdt.snapshots.columns.locationUrl),
    fileSizeBytes: integer(
      tableNames.haex.crdt.snapshots.columns.fileSizeBytes,
    ),
  },
)

export const haexCrdtConfigs = sqliteTable(tableNames.haex.crdt.configs.name, {
  key: text(tableNames.haex.crdt.configs.columns.key).primaryKey(),
  type: text(tableNames.haex.crdt.configs.columns.type).notNull(),
  value: text(tableNames.haex.crdt.configs.columns.value),
})

/**
 * Sync Status Table (WITHOUT CRDT - local-only metadata)
 * Tracks sync progress for each backend
 */
export const haexCrdtSyncStatus = sqliteTable(
  tableNames.haex.crdt.sync_status.name,
  {
    id: text(tableNames.haex.crdt.sync_status.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    backendId: text(tableNames.haex.crdt.sync_status.columns.backendId).notNull(),
    // Last server createdAt timestamp received from pull (ISO 8601)
    lastPullCreatedAt: text(tableNames.haex.crdt.sync_status.columns.lastPullCreatedAt),
    // Last HLC timestamp pushed to server
    lastPushHlcTimestamp: text(tableNames.haex.crdt.sync_status.columns.lastPushHlcTimestamp),
    // Last successful sync timestamp
    lastSyncAt: text(tableNames.haex.crdt.sync_status.columns.lastSyncAt),
    // Sync error message if any
    error: text(tableNames.haex.crdt.sync_status.columns.error),
  },
)
export type InsertHaexCrdtSyncStatus = typeof haexCrdtSyncStatus.$inferInsert
export type SelectHaexCrdtSyncStatus = typeof haexCrdtSyncStatus.$inferSelect
