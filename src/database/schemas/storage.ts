import { sql } from 'drizzle-orm'
import {
  type AnySQLiteColumn,
  integer,
  sqliteTable,
  text,
  uniqueIndex,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

/**
 * Storage Backends (WITH CRDT - synced between devices)
 * Central registry for S3-compatible storage backends.
 * Multiple extensions can share the same backend without the user
 * having to configure it multiple times.
 *
 * Note: Config is stored as plain JSON (not encrypted) because
 * SQLite database is already encrypted with SQLCipher at file level.
 *
 * Note: CRDT columns and UNIQUE index WHERE conditions are added automatically
 * by the Rust CrdtTransformer.
 *
 * Supported types: 's3' (later: 'webdav', etc.)
 * Config structure depends on type - validated at runtime.
 */
export const haexStorageBackends = sqliteTable(
  tableNames.haex.storage_backends.name,
  {
    id: text(tableNames.haex.storage_backends.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    type: text(tableNames.haex.storage_backends.columns.type).notNull(), // 's3', später 'webdav', etc.
    name: text(tableNames.haex.storage_backends.columns.name).notNull(),
    // Config as JSON - structure depends on type, validated at runtime
    // S3: { endpoint?, bucket, region, accessKeyId, secretAccessKey, pathStyle? }
    // WebDAV (future): { url, username, password }
    config: text(tableNames.haex.storage_backends.columns.config, { mode: 'json' })
      .notNull()
      .$type<Record<string, unknown>>(),
    enabled: integer(tableNames.haex.storage_backends.columns.enabled, {
      mode: 'boolean',
    })
      .default(true)
      .notNull(),
    parentBackendId: text(tableNames.haex.storage_backends.columns.parentBackendId).references(
      (): AnySQLiteColumn => haexStorageBackends.id,
      { onDelete: 'cascade' },
    ),
    // originType distinguishes the row's provenance:
    //   'owned'             — this device owns the backend (original row, parentBackendId IS NULL)
    //   'shared_from_space' — this row was created because another Space participant shared their
    //                         backend with us; parentBackendId references the owner-side row and
    //                         sharePrefix/shareAccessFlags describe the granted scope.
    originType: text(tableNames.haex.storage_backends.columns.originType)
      .$type<'owned' | 'shared_from_space'>()
      .notNull()
      .default('owned'),
    sharePrefix: text(tableNames.haex.storage_backends.columns.sharePrefix),
    // shareAccessFlags bitmap:  bit0=list  bit1=get  bit2=put  bit3=delete
    // NULL only valid when originType='owned'.
    shareAccessFlags: integer(tableNames.haex.storage_backends.columns.shareAccessFlags),
    createdAt: text(tableNames.haex.storage_backends.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_storage_backends_name_unique').on(table.name),
  ],
)
export type InsertHaexStorageBackends = typeof haexStorageBackends.$inferInsert
export type SelectHaexStorageBackends = typeof haexStorageBackends.$inferSelect
