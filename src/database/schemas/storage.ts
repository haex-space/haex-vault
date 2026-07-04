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
 * S3 Backends (WITH CRDT - synced between devices)
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
 * Config structure:
 *   { endpoint?, bucket, region, accessKeyId, secretAccessKey, pathStyle? }
 */
export const haexS3Backends = sqliteTable(
  tableNames.haex.s3_backends.name,
  {
    id: text(tableNames.haex.s3_backends.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    type: text(tableNames.haex.s3_backends.columns.type).notNull(),
    name: text(tableNames.haex.s3_backends.columns.name).notNull(),
    // Config as JSON - S3-specific shape, validated at runtime
    // { endpoint?, bucket, region, accessKeyId, secretAccessKey, pathStyle? }
    config: text(tableNames.haex.s3_backends.columns.config, { mode: 'json' })
      .notNull()
      .$type<Record<string, unknown>>(),
    enabled: integer(tableNames.haex.s3_backends.columns.enabled, {
      mode: 'boolean',
    })
      .default(true)
      .notNull(),
    parentBackendId: text(tableNames.haex.s3_backends.columns.parentBackendId).references(
      (): AnySQLiteColumn => haexS3Backends.id,
      { onDelete: 'cascade' },
    ),
    // originType distinguishes the row's provenance:
    //   'owned'             — this device owns the backend (original row, parentBackendId IS NULL)
    //   'shared_from_space' — this row was created because another Space participant shared their
    //                         backend with us; parentBackendId references the owner-side row and
    //                         sharePrefix/shareAccessFlags describe the granted scope.
    originType: text(tableNames.haex.s3_backends.columns.originType)
      .$type<'owned' | 'shared_from_space'>()
      .notNull()
      .default('owned'),
    sharePrefix: text(tableNames.haex.s3_backends.columns.sharePrefix),
    // shareAccessFlags bitmap:  bit0=list  bit1=get  bit2=put  bit3=delete
    // NULL only valid when originType='owned'.
    shareAccessFlags: integer(tableNames.haex.s3_backends.columns.shareAccessFlags),
    createdAt: text(tableNames.haex.s3_backends.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_s3_backends_name_unique').on(table.name),
  ],
)
export type InsertHaexS3Backends = typeof haexS3Backends.$inferInsert
export type SelectHaexS3Backends = typeof haexS3Backends.$inferSelect
