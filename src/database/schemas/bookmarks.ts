import { sql } from 'drizzle-orm'
import {
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// ---------------------------------------------------------------------------
// Bookmarks — structured, syncable bookmark storage for haex-pass-browser.
//
// A collection is the independence boundary: browsers pointed at the same
// collection converge (canonical roots toolbar<->toolbar etc.), different
// collections never mix. Convergence itself is handled by the vault's
// automatic CRDT layer (see plans/001-add-bookmarks-table-and-bridge.md) —
// these tables carry no manual CRDT columns and no soft-delete column;
// hard-DELETE produces a tombstone via the existing BEFORE-DELETE trigger.
// ---------------------------------------------------------------------------

export const haexBookmarkCollections = sqliteTable(
  tableNames.haex.bookmark_collections.name,
  {
    id: text(tableNames.haex.bookmark_collections.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.bookmark_collections.columns.name),
    createdAt: text(tableNames.haex.bookmark_collections.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: text(tableNames.haex.bookmark_collections.columns.updatedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexBookmarkCollections = typeof haexBookmarkCollections.$inferInsert
export type SelectHaexBookmarkCollections = typeof haexBookmarkCollections.$inferSelect

// One node per row (folder / bookmark / separator), always scoped to a
// collection. parentId=null marks a canonical root node (rootKind set).
export const haexBookmarks = sqliteTable(
  tableNames.haex.bookmarks.name,
  {
    id: text(tableNames.haex.bookmarks.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    collectionId: text(tableNames.haex.bookmarks.columns.collectionId)
      .notNull()
      .references((): AnySQLiteColumn => haexBookmarkCollections.id, { onDelete: 'cascade' }),
    parentId: text(tableNames.haex.bookmarks.columns.parentId).references(
      (): AnySQLiteColumn => haexBookmarks.id,
      { onDelete: 'cascade' },
    ),
    rootKind: text(tableNames.haex.bookmarks.columns.rootKind),
    kind: text(tableNames.haex.bookmarks.columns.kind).notNull(),
    title: text(tableNames.haex.bookmarks.columns.title),
    url: text(tableNames.haex.bookmarks.columns.url),
    position: integer(tableNames.haex.bookmarks.columns.position).notNull(),
    createdAt: text(tableNames.haex.bookmarks.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: text(tableNames.haex.bookmarks.columns.updatedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
)
export type InsertHaexBookmarks = typeof haexBookmarks.$inferInsert
export type SelectHaexBookmarks = typeof haexBookmarks.$inferSelect

// Registry for onboarding display ("Private · 3 devices"). Pure metadata —
// no sync or security logic depends on it.
export const haexBookmarkDevices = sqliteTable(
  tableNames.haex.bookmark_devices.name,
  {
    id: text(tableNames.haex.bookmark_devices.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    collectionId: text(tableNames.haex.bookmark_devices.columns.collectionId)
      .notNull()
      .references((): AnySQLiteColumn => haexBookmarkCollections.id, { onDelete: 'cascade' }),
    replicaId: text(tableNames.haex.bookmark_devices.columns.replicaId).notNull(),
    deviceLabel: text(tableNames.haex.bookmark_devices.columns.deviceLabel).notNull(),
    browserFamily: text(tableNames.haex.bookmark_devices.columns.browserFamily).notNull(),
    lastSeenAt: text(tableNames.haex.bookmark_devices.columns.lastSeenAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_bookmark_devices_collection_replica_unique').on(
      table.collectionId,
      table.replicaId,
    ),
  ],
)
export type InsertHaexBookmarkDevices = typeof haexBookmarkDevices.$inferInsert
export type SelectHaexBookmarkDevices = typeof haexBookmarkDevices.$inferSelect
