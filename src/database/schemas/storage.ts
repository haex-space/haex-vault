import { sql } from 'drizzle-orm'
import {
  integer,
  sqliteTable,
  text,
  uniqueIndex,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'
import { withCrdtColumns } from './haex'

/**
 * Storage Backends (WITH CRDT - synced between devices)
 * Central registry for S3-compatible storage backends.
 * Multiple extensions can share the same backend without the user
 * having to configure it multiple times.
 *
 * Note: Config is stored as plain JSON (not encrypted) because
 * SQLite database is already encrypted with SQLCipher at file level.
 *
 * Supported types: 's3' (later: 'webdav', etc.)
 * Config structure depends on type - validated at runtime.
 */
export const haexStorageBackends = sqliteTable(
  tableNames.haex.storage_backends.name,
  withCrdtColumns({
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
    createdAt: text(tableNames.haex.storage_backends.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  }),
  (table) => [
    // Name must be unique (excluding tombstoned entries)
    uniqueIndex('haex_storage_backends_name_unique')
      .on(table.name)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexStorageBackends = typeof haexStorageBackends.$inferInsert
export type SelectHaexStorageBackends = typeof haexStorageBackends.$inferSelect
