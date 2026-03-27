import {
  blob,
  integer,
  primaryKey,
  sqliteTable,
  text,
} from 'drizzle-orm/sqlite-core'

// ---------------------------------------------------------------------------
// MLS StorageProvider — local-only, no CRDT sync
// Generic key-value tables for OpenMLS state persistence (group state, keys, proposals)
// ---------------------------------------------------------------------------

export const haexMlsValuesNoSync = sqliteTable(
  'haex_mls_values_no_sync',
  {
    storeType: text('store_type').notNull(),
    keyBytes: blob('key_bytes', { mode: 'buffer' }).notNull(),
    valueBlob: blob('value_blob', { mode: 'buffer' }).notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.storeType, table.keyBytes] }),
  ],
)

export const haexMlsListNoSync = sqliteTable(
  'haex_mls_list_no_sync',
  {
    storeType: text('store_type').notNull(),
    keyBytes: blob('key_bytes', { mode: 'buffer' }).notNull(),
    indexNum: integer('index_num').notNull(),
    valueBlob: blob('value_blob', { mode: 'buffer' }).notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.storeType, table.keyBytes, table.indexNum] }),
  ],
)

export const haexMlsEpochKeyPairsNoSync = sqliteTable(
  'haex_mls_epoch_key_pairs_no_sync',
  {
    groupId: blob('group_id', { mode: 'buffer' }).notNull(),
    epochBytes: blob('epoch_bytes', { mode: 'buffer' }).notNull(),
    leafIndex: integer('leaf_index').notNull(),
    valueBlob: blob('value_blob', { mode: 'buffer' }).notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.groupId, table.epochBytes, table.leafIndex] }),
  ],
)
