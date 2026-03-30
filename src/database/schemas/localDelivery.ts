import { sqliteTable, text, integer, index, blob } from 'drizzle-orm/sqlite-core'
import { sql } from 'drizzle-orm'
import tableNames from '../tableNames.json'
import { haexSpaces } from './haex'

// ---------------------------------------------------------------------------
// Local Delivery Service — MLS message buffering for local spaces (_no_sync)
// ---------------------------------------------------------------------------

export const haexLocalDsMessages = sqliteTable(
  tableNames.haex.local_ds_messages.name,
  {
    id: integer(tableNames.haex.local_ds_messages.columns.id).primaryKey({ autoIncrement: true }),
    spaceId: text(tableNames.haex.local_ds_messages.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    senderDid: text(tableNames.haex.local_ds_messages.columns.senderDid).notNull(),
    messageType: text(tableNames.haex.local_ds_messages.columns.messageType).notNull(),
    messageBlob: blob(tableNames.haex.local_ds_messages.columns.messageBlob, { mode: 'buffer' }).notNull(),
    createdAt: text(tableNames.haex.local_ds_messages.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_messages_space_idx').on(table.spaceId),
  ],
)
export type InsertHaexLocalDsMessages = typeof haexLocalDsMessages.$inferInsert
export type SelectHaexLocalDsMessages = typeof haexLocalDsMessages.$inferSelect

export const haexLocalDsKeyPackages = sqliteTable(
  tableNames.haex.local_ds_key_packages.name,
  {
    id: text(tableNames.haex.local_ds_key_packages.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_ds_key_packages.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    targetDid: text(tableNames.haex.local_ds_key_packages.columns.targetDid).notNull(),
    packageBlob: blob(tableNames.haex.local_ds_key_packages.columns.packageBlob, { mode: 'buffer' }).notNull(),
    createdAt: text(tableNames.haex.local_ds_key_packages.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_key_packages_space_did_idx').on(table.spaceId, table.targetDid),
  ],
)
export type InsertHaexLocalDsKeyPackages = typeof haexLocalDsKeyPackages.$inferInsert
export type SelectHaexLocalDsKeyPackages = typeof haexLocalDsKeyPackages.$inferSelect

export const haexLocalDsWelcomes = sqliteTable(
  tableNames.haex.local_ds_welcomes.name,
  {
    id: text(tableNames.haex.local_ds_welcomes.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_ds_welcomes.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    recipientDid: text(tableNames.haex.local_ds_welcomes.columns.recipientDid).notNull(),
    welcomeBlob: blob(tableNames.haex.local_ds_welcomes.columns.welcomeBlob, { mode: 'buffer' }).notNull(),
    consumed: integer(tableNames.haex.local_ds_welcomes.columns.consumed).default(0),
    createdAt: text(tableNames.haex.local_ds_welcomes.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_welcomes_recipient_idx').on(table.spaceId, table.recipientDid),
  ],
)
export type InsertHaexLocalDsWelcomes = typeof haexLocalDsWelcomes.$inferInsert
export type SelectHaexLocalDsWelcomes = typeof haexLocalDsWelcomes.$inferSelect

export const haexLocalDsPendingCommits = sqliteTable(
  tableNames.haex.local_ds_pending_commits.name,
  {
    id: text(tableNames.haex.local_ds_pending_commits.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_ds_pending_commits.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    commitBlob: blob(tableNames.haex.local_ds_pending_commits.columns.commitBlob, { mode: 'buffer' }).notNull(),
    deliveredTo: text(tableNames.haex.local_ds_pending_commits.columns.deliveredTo).default('[]'),
    createdAt: text(tableNames.haex.local_ds_pending_commits.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_ds_pending_commits_space_idx').on(table.spaceId),
  ],
)
export type InsertHaexLocalDsPendingCommits = typeof haexLocalDsPendingCommits.$inferInsert
export type SelectHaexLocalDsPendingCommits = typeof haexLocalDsPendingCommits.$inferSelect
