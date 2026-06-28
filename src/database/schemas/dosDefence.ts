import { check, integer, sqliteTable, text } from 'drizzle-orm/sqlite-core'
import { sql } from 'drizzle-orm'
import tableNames from '@/database/tableNames.json'

/**
 * DoS-defence runtime state for the local Leader. Singleton (`CHECK id = 1`)
 * because each device tracks its own flood state — not synced across owner
 * devices.
 *
 * `_no_sync` suffix: high-frequency updates during a DDoS would otherwise
 * flood the owner-sync pipeline. The companion config (thresholds, policy)
 * lives in `haex_vault_settings` under the `dosDefence.*` prefix — that one
 * IS synced because it represents user choice, not transient runtime state.
 *
 * See `docs/plans/2026-06-13-leader-reject-rate-limit.md` §Phase 3.
 */
export const haexDosDefenceStateNoSync = sqliteTable(
  tableNames.haex.dos_defence_state_no_sync.name,
  {
    id: integer(tableNames.haex.dos_defence_state_no_sync.columns.id).primaryKey(),
    /** Discriminator: `quiet` | `single_source` | `ddos`. */
    floodMode: text(tableNames.haex.dos_defence_state_no_sync.columns.floodMode)
      .notNull()
      .default('quiet'),
    /** Source DID when `floodMode = 'single_source'`; NULL otherwise. */
    floodModeSource: text(tableNames.haex.dos_defence_state_no_sync.columns.floodModeSource),
    /** RFC3339 timestamp at which auto-expiry flips DDoS back to quiet; NULL outside DDoS. */
    ddosExpiresAt: text(tableNames.haex.dos_defence_state_no_sync.columns.ddosExpiresAt),
    /** RFC3339 timestamp of the last transition write. */
    updatedAt: text(tableNames.haex.dos_defence_state_no_sync.columns.updatedAt).notNull(),
  },
  (table) => [check('haex_dos_defence_state_singleton', sql`${table.id} = 1`)],
)

export type InsertHaexDosDefenceState = typeof haexDosDefenceStateNoSync.$inferInsert
export type SelectHaexDosDefenceState = typeof haexDosDefenceStateNoSync.$inferSelect
