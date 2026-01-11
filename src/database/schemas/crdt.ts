import {
  index,
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'
import { haexExtensions } from './haex'

export const crdtTableNames = tableNames.haex.crdt

// Most CRDT metadata tables removed:
// - haexCrdtChanges: Sync now works by scanning actual tables directly
// - haexCrdtSnapshots: Not used
// - haexCrdtSyncStatus: Replaced by timestamps in haexSyncBackends table

/**
 * CRDT Configuration (WITHOUT CRDT - local-only metadata)
 * Stores HLC node ID and last timestamp for this device
 * Used by the HLC service to maintain consistent logical time
 */
export const haexCrdtConfigs = sqliteTable(crdtTableNames.configs.name, {
  key: text(crdtTableNames.configs.columns.key).primaryKey(),
  type: text(crdtTableNames.configs.columns.type).notNull(),
  value: text(crdtTableNames.configs.columns.value).notNull(),
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
  crdtTableNames.dirty_tables.name,
  {
    tableName: text(crdtTableNames.dirty_tables.columns.tableName).primaryKey(),
    lastModified: text(crdtTableNames.dirty_tables.columns.lastModified)
      .notNull()
      .$defaultFn(() => new Date().toISOString()),
  },
)
export type InsertHaexCrdtDirtyTables = typeof haexCrdtDirtyTables.$inferInsert
export type SelectHaexCrdtDirtyTables = typeof haexCrdtDirtyTables.$inferSelect

/**
 * CRDT Conflicts (WITHOUT CRDT - local-only conflict tracking)
 * Tracks synchronization conflicts that require user resolution
 * When remote changes violate UNIQUE constraints, both versions are stored here
 * User can then decide which version to keep (local or remote)
 */
export const haexCrdtConflicts = sqliteTable(
  crdtTableNames.conflicts.name,
  {
    id: text(crdtTableNames.conflicts.columns.id)
      .primaryKey()
      .$defaultFn(() => crypto.randomUUID()),
    tableName: text(crdtTableNames.conflicts.columns.tableName).notNull(),
    conflictType: text(crdtTableNames.conflicts.columns.conflictType).notNull(),
    localRowId: text(crdtTableNames.conflicts.columns.localRowId).notNull(),
    remoteRowId: text(crdtTableNames.conflicts.columns.remoteRowId).notNull(),
    localRowData: text(crdtTableNames.conflicts.columns.localRowData).notNull(),
    remoteRowData: text(crdtTableNames.conflicts.columns.remoteRowData).notNull(),
    localTimestamp: text(crdtTableNames.conflicts.columns.localTimestamp).notNull(),
    remoteTimestamp: text(crdtTableNames.conflicts.columns.remoteTimestamp).notNull(),
    conflictKey: text(crdtTableNames.conflicts.columns.conflictKey).notNull(),
    detectedAt: text(crdtTableNames.conflicts.columns.detectedAt)
      .notNull()
      .$defaultFn(() => new Date().toISOString()),
    resolved: integer(crdtTableNames.conflicts.columns.resolved, { mode: 'boolean' })
      .notNull()
      .default(false),
    resolution: text(crdtTableNames.conflicts.columns.resolution),
    resolvedAt: text(crdtTableNames.conflicts.columns.resolvedAt),
  },
  (table) => [
    index('haex_crdt_conflicts_no_sync_table_name_idx').on(table.tableName),
    index('haex_crdt_conflicts_no_sync_resolved_idx').on(table.resolved),
  ],
)
export type InsertHaexCrdtConflicts = typeof haexCrdtConflicts.$inferInsert
export type SelectHaexCrdtConflicts = typeof haexCrdtConflicts.$inferSelect

/**
 * Core Migrations (WITHOUT CRDT - local-only metadata)
 * Tracks which core system migrations have been applied to this vault
 * Unlike extension migrations, these are NOT synchronized between devices
 * Each device applies core migrations independently from the bundled migration files
 *
 * The migrationContent contains the complete .sql file with all statements
 * separated by '--> statement-breakpoint' markers (Drizzle format)
 *
 * IMPORTANT: This table is ALWAYS created by Rust (bootstrapping in migrations.rs)
 * DO NOT generate migrations for this table via Drizzle!
 * The SQL DEFAULT for appliedAt is set in Rust: DEFAULT (CURRENT_TIMESTAMP)
 */
export const haexCrdtMigrations = sqliteTable(
  crdtTableNames.migrations.name,
  {
    id: text(crdtTableNames.migrations.columns.id).primaryKey(),
    extensionId: text(crdtTableNames.migrations.columns.extensionId).references(
      (): AnySQLiteColumn => haexExtensions.id,
      { onDelete: 'cascade' },
    ),
    migrationName: text(crdtTableNames.migrations.columns.migrationName).notNull(),
    migrationContent: text(crdtTableNames.migrations.columns.migrationContent).notNull(),
    appliedAt: text(crdtTableNames.migrations.columns.appliedAt).notNull(),
  },
  (table) => [
    // Unique index on (extensionId, migrationName) - each extension can have its own migrations
    // Core migrations have extensionId = NULL, extension migrations have their extension_id
    uniqueIndex('haex_crdt_migrations_no_sync_ext_name_unique').on(
      table.extensionId,
      table.migrationName,
    ),
  ],
)
export type InsertHaexCrdtMigrations = typeof haexCrdtMigrations.$inferInsert
export type SelectHaexCrdtMigrations = typeof haexCrdtMigrations.$inferSelect
