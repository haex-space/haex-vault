import { sql } from 'drizzle-orm'
import { integer, sqliteTable, text } from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

export const haexMarketplaces = sqliteTable(
  tableNames.haex.marketplaces.name,
  {
    id: text(tableNames.haex.marketplaces.columns.id).primaryKey(),
    name: text(tableNames.haex.marketplaces.columns.name).notNull(),
    baseUrl: text(tableNames.haex.marketplaces.columns.baseUrl).notNull(),
    enabled: integer(tableNames.haex.marketplaces.columns.enabled, { mode: 'boolean' }).notNull().default(true),
    isDefault: integer(tableNames.haex.marketplaces.columns.isDefault, { mode: 'boolean' }).notNull().default(false),
    sortOrder: integer(tableNames.haex.marketplaces.columns.sortOrder).notNull().default(100),
    authType: text(tableNames.haex.marketplaces.columns.authType).notNull().default('none'),
    authToken: text(tableNames.haex.marketplaces.columns.authToken),
    authUsername: text(tableNames.haex.marketplaces.columns.authUsername),
    authPassword: text(tableNames.haex.marketplaces.columns.authPassword),
    authIdentityId: text(tableNames.haex.marketplaces.columns.authIdentityId),
    createdAt: text(tableNames.haex.marketplaces.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
    updatedAt: text(tableNames.haex.marketplaces.columns.updatedAt).default(sql`(CURRENT_TIMESTAMP)`),
  },
)

export type InsertHaexMarketplaces = typeof haexMarketplaces.$inferInsert
export type SelectHaexMarketplaces = typeof haexMarketplaces.$inferSelect
