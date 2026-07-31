import { sql } from 'drizzle-orm'
import { check, index, sqliteTable, text } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'
import { haexSpaces } from './spaces'

// ---------------------------------------------------------------------------
// Space UCAN Grants — local, non-CRDT-synced bilateral UCAN storage: tracks
// both the grants this device issued to other space members and the grants
// it received from them. `_no_sync`: never touched by CRDT machinery, no
// haex_hlc / haex_column_hlcs / haex_column_sigs meta columns.
// ---------------------------------------------------------------------------

export const haexSpaceUcanGrants = sqliteTable(
  tableNames.haex.space_ucan_grants_no_sync.name,
  {
    id: text(tableNames.haex.space_ucan_grants_no_sync.columns.id).primaryKey(),
    spaceId: text(tableNames.haex.space_ucan_grants_no_sync.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id, { onDelete: 'cascade' }),
    issuerDid: text(tableNames.haex.space_ucan_grants_no_sync.columns.issuerDid).notNull(),
    audienceDid: text(tableNames.haex.space_ucan_grants_no_sync.columns.audienceDid).notNull(),
    ucanToken: text(tableNames.haex.space_ucan_grants_no_sync.columns.ucanToken).notNull(),
    role: text(tableNames.haex.space_ucan_grants_no_sync.columns.role).notNull(), // 'issued' | 'received'
    createdAt: text(tableNames.haex.space_ucan_grants_no_sync.columns.createdAt).notNull(),
    revokedAt: text(tableNames.haex.space_ucan_grants_no_sync.columns.revokedAt),
  },
  (table) => [
    index('haex_space_ucan_grants_lookup').on(table.spaceId, table.audienceDid, table.revokedAt),
    check('haex_space_ucan_grants_no_sync_role_check', sql`role IN ('issued','received')`),
  ],
)
export type InsertHaexSpaceUcanGrants = typeof haexSpaceUcanGrants.$inferInsert
export type SelectHaexSpaceUcanGrants = typeof haexSpaceUcanGrants.$inferSelect
