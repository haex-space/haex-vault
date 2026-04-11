import { sqliteTable, text, integer, index, blob } from 'drizzle-orm/sqlite-core'
import { sql } from 'drizzle-orm'
import tableNames from '../tableNames.json'
import { haexSpaces } from './spaces'

// ---------------------------------------------------------------------------
// Local Delivery Service — MLS message buffering for local spaces (_no_sync)
// ---------------------------------------------------------------------------

export const haexLocalDeliveryMessages = sqliteTable(
  tableNames.haex.local_delivery_messages.name,
  {
    id: integer(tableNames.haex.local_delivery_messages.columns.id).primaryKey({ autoIncrement: true }),
    spaceId: text(tableNames.haex.local_delivery_messages.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    senderDid: text(tableNames.haex.local_delivery_messages.columns.senderDid).notNull(),
    messageType: text(tableNames.haex.local_delivery_messages.columns.messageType).notNull(),
    messageBlob: blob(tableNames.haex.local_delivery_messages.columns.messageBlob, { mode: 'buffer' }).notNull(),
    createdAt: text(tableNames.haex.local_delivery_messages.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_delivery_messages_space_idx').on(table.spaceId),
  ],
)
export type InsertHaexLocalDeliveryMessages = typeof haexLocalDeliveryMessages.$inferInsert
export type SelectHaexLocalDeliveryMessages = typeof haexLocalDeliveryMessages.$inferSelect

export const haexLocalDeliveryKeyPackages = sqliteTable(
  tableNames.haex.local_delivery_key_packages.name,
  {
    id: text(tableNames.haex.local_delivery_key_packages.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_delivery_key_packages.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    targetDid: text(tableNames.haex.local_delivery_key_packages.columns.targetDid).notNull(),
    packageBlob: blob(tableNames.haex.local_delivery_key_packages.columns.packageBlob, { mode: 'buffer' }).notNull(),
    createdAt: text(tableNames.haex.local_delivery_key_packages.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_delivery_key_packages_space_did_idx').on(table.spaceId, table.targetDid),
  ],
)
export type InsertHaexLocalDeliveryKeyPackages = typeof haexLocalDeliveryKeyPackages.$inferInsert
export type SelectHaexLocalDeliveryKeyPackages = typeof haexLocalDeliveryKeyPackages.$inferSelect

export const haexLocalDeliveryWelcomes = sqliteTable(
  tableNames.haex.local_delivery_welcomes.name,
  {
    id: text(tableNames.haex.local_delivery_welcomes.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_delivery_welcomes.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    recipientDid: text(tableNames.haex.local_delivery_welcomes.columns.recipientDid).notNull(),
    welcomeBlob: blob(tableNames.haex.local_delivery_welcomes.columns.welcomeBlob, { mode: 'buffer' }).notNull(),
    consumed: integer(tableNames.haex.local_delivery_welcomes.columns.consumed).default(0),
    createdAt: text(tableNames.haex.local_delivery_welcomes.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_delivery_welcomes_recipient_idx').on(table.spaceId, table.recipientDid),
  ],
)
export type InsertHaexLocalDeliveryWelcomes = typeof haexLocalDeliveryWelcomes.$inferInsert
export type SelectHaexLocalDeliveryWelcomes = typeof haexLocalDeliveryWelcomes.$inferSelect

export const haexLocalDeliveryPendingCommits = sqliteTable(
  tableNames.haex.local_delivery_pending_commits.name,
  {
    id: text(tableNames.haex.local_delivery_pending_commits.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    spaceId: text(tableNames.haex.local_delivery_pending_commits.columns.spaceId)
      .notNull()
      .references(() => haexSpaces.id),
    messageId: integer(tableNames.haex.local_delivery_pending_commits.columns.messageId).notNull(),
    expectedDids: text(tableNames.haex.local_delivery_pending_commits.columns.expectedDids).notNull().default('[]'),
    ackedDids: text(tableNames.haex.local_delivery_pending_commits.columns.ackedDids).notNull().default('[]'),
    createdAt: text(tableNames.haex.local_delivery_pending_commits.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
  (table) => [
    index('haex_local_delivery_pending_commits_space_idx').on(table.spaceId),
  ],
)
export type InsertHaexLocalDeliveryPendingCommits = typeof haexLocalDeliveryPendingCommits.$inferInsert
export type SelectHaexLocalDeliveryPendingCommits = typeof haexLocalDeliveryPendingCommits.$inferSelect
