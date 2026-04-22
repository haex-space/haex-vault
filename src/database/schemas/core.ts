import { sql } from 'drizzle-orm'
import {
  check,
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

// Note: CRDT columns (haex_hlc, haex_column_hlcs) are added automatically by the
// Rust CrdtTransformer when CREATE TABLE is executed. DELETE on these tables is
// logged to `haex_deleted_rows` via a BEFORE-DELETE trigger (no tombstone column).

export const haexVaultSettings = sqliteTable(
  tableNames.haex.vault_settings.name,
  {
    id: text(tableNames.haex.vault_settings.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    key: text(tableNames.haex.vault_settings.columns.key).notNull(),
    value: text(tableNames.haex.vault_settings.columns.value),
    deviceId: text(tableNames.haex.vault_settings.columns.deviceId),
  },
  (table) => [
    uniqueIndex('haex_vault_settings_key_device_unique').on(table.key, table.deviceId),
  ],
)
export type InsertHaexVaultSettings = typeof haexVaultSettings.$inferInsert
export type SelectHaexVaultSettings = typeof haexVaultSettings.$inferSelect

// ---------------------------------------------------------------------------
// Extensions — installed haextensions
// ---------------------------------------------------------------------------

export const haexExtensions = sqliteTable(
  tableNames.haex.extensions.name,
  {
    id: text()
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    public_key: text().notNull(),
    name: text().notNull(),
    version: text().notNull(),
    author: text(),
    description: text(),
    entry: text().default('index.html'),
    homepage: text(),
    enabled: integer({ mode: 'boolean' }).default(true),
    icon: text(),
    signature: text().notNull(),
    single_instance: integer({ mode: 'boolean' }).default(false),
    display_mode: text().default('auto'),
    // i18n overrides: { "de": { "name": "...", "description": "..." }, ... }
    i18n: text({ mode: 'json' }).$type<Record<string, { name?: string; description?: string }>>(),
    // path to dev extension project folder (if set, this is a dev extension)
    dev_path: text(),
    createdAt: text(tableNames.haex.extensions.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.extensions.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_extensions_public_key_name_unique').on(table.public_key, table.name),
  ],
)
export type InsertHaexExtensions = typeof haexExtensions.$inferInsert
export type SelectHaexExtensions = typeof haexExtensions.$inferSelect

export const haexExtensionPermissions = sqliteTable(
  tableNames.haex.extension_permissions.name,
  {
    id: text()
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    extensionId: text(tableNames.haex.extension_permissions.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    resourceType: text('resource_type', {
      enum: ['fs', 'web', 'db', 'shell'],
    }),
    action: text({ enum: ['read', 'write'] }),
    target: text(),
    constraints: text({ mode: 'json' }),
    status: text({ enum: ['ask', 'granted', 'denied'] })
      .notNull()
      .default('denied'),
    createdAt: text('created_at').default(sql`(CURRENT_TIMESTAMP)`),
    updateAt: integer('updated_at', { mode: 'timestamp' }).$onUpdate(
      () => new Date(),
    ),
  },
  (table) => [
    uniqueIndex('haex_extension_permissions_extension_id_resource_type_action_target_unique')
      .on(table.extensionId, table.resourceType, table.action, table.target),
  ],
)
export type InserthaexExtensionPermissions =
  typeof haexExtensionPermissions.$inferInsert
export type SelecthaexExtensionPermissions =
  typeof haexExtensionPermissions.$inferSelect

// ---------------------------------------------------------------------------
// Logs — structured logging for system processes and extensions
// ---------------------------------------------------------------------------

export const haexLogs = sqliteTable(
  tableNames.haex.logs.name,
  {
    id: text(tableNames.haex.logs.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    timestamp: text(tableNames.haex.logs.columns.timestamp).notNull(),
    level: text(tableNames.haex.logs.columns.level, {
      enum: ['debug', 'info', 'warn', 'error'],
    }).notNull(),
    source: text(tableNames.haex.logs.columns.source).notNull(),
    extensionId: text(tableNames.haex.logs.columns.extensionId)
      .references((): AnySQLiteColumn => haexExtensions.id, { onDelete: 'cascade' }),
    message: text(tableNames.haex.logs.columns.message).notNull(),
    metadata: text(tableNames.haex.logs.columns.metadata, { mode: 'json' }),
    deviceId: text(tableNames.haex.logs.columns.deviceId).notNull(),
  },
)
export type InsertHaexLogs = typeof haexLogs.$inferInsert
export type SelectHaexLogs = typeof haexLogs.$inferSelect

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

export const haexNotifications = sqliteTable(
  tableNames.haex.notifications.name,
  {
    id: text()
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    alt: text(),
    date: text(),
    icon: text(),
    image: text(),
    read: integer({ mode: 'boolean' }),
    source: text(),
    text: text(),
    title: text(),
    type: text({
      enum: ['error', 'success', 'warning', 'info', 'log'],
    }).notNull(),
  },
)
export type InsertHaexNotifications = typeof haexNotifications.$inferInsert
export type SelectHaexNotifications = typeof haexNotifications.$inferSelect

// ---------------------------------------------------------------------------
// Workspaces & Desktop Items
// ---------------------------------------------------------------------------

export const haexWorkspaces = sqliteTable(
  tableNames.haex.workspaces.name,
  {
    id: text(tableNames.haex.workspaces.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    deviceId: text(tableNames.haex.workspaces.columns.deviceId).notNull(),
    name: text(tableNames.haex.workspaces.columns.name).notNull(),
    position: integer(tableNames.haex.workspaces.columns.position)
      .notNull()
      .default(0),
    background: text(),
  },
  (table) => [
    uniqueIndex('haex_workspaces_device_position_unique').on(table.deviceId, table.position),
  ],
)
export type InsertHaexWorkspaces = typeof haexWorkspaces.$inferInsert
export type SelectHaexWorkspaces = typeof haexWorkspaces.$inferSelect

export const haexDesktopItems = sqliteTable(
  tableNames.haex.desktop_items.name,
  {
    id: text(tableNames.haex.desktop_items.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    workspaceId: text(tableNames.haex.desktop_items.columns.workspaceId)
      .notNull()
      .references(() => haexWorkspaces.id, { onDelete: 'cascade' }),
    itemType: text(tableNames.haex.desktop_items.columns.itemType, {
      enum: ['system', 'extension', 'file', 'folder'],
    }).notNull(),
    extensionId: text(
      tableNames.haex.desktop_items.columns.extensionId,
    ).references((): AnySQLiteColumn => haexExtensions.id, {
      onDelete: 'cascade',
    }),
    systemWindowId: text(tableNames.haex.desktop_items.columns.systemWindowId),
    positionX: integer(tableNames.haex.desktop_items.columns.positionX)
      .notNull()
      .default(0),
    positionY: integer(tableNames.haex.desktop_items.columns.positionY)
      .notNull()
      .default(0),
  },
  (table) => [
    check(
      'item_reference',
      sql`(${table.itemType} = 'extension' AND ${table.extensionId} IS NOT NULL AND ${table.systemWindowId} IS NULL) OR (${table.itemType} = 'system' AND ${table.systemWindowId} IS NOT NULL AND ${table.extensionId} IS NULL) OR (${table.itemType} = 'file' AND ${table.systemWindowId} IS NOT NULL AND ${table.extensionId} IS NULL) OR (${table.itemType} = 'folder' AND ${table.systemWindowId} IS NOT NULL AND ${table.extensionId} IS NULL)`,
    ),
  ],
)
export type InsertHaexDesktopItems = typeof haexDesktopItems.$inferInsert
export type SelectHaexDesktopItems = typeof haexDesktopItems.$inferSelect

// ---------------------------------------------------------------------------
// Extension Migrations
// ---------------------------------------------------------------------------

export const haexExtensionMigrations = sqliteTable(
  tableNames.haex.extension_migrations.name,
  {
    id: text(tableNames.haex.extension_migrations.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    extensionId: text(tableNames.haex.extension_migrations.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    extensionVersion: text(tableNames.haex.extension_migrations.columns.extensionVersion).notNull(),
    migrationName: text(tableNames.haex.extension_migrations.columns.migrationName).notNull(),
    sqlStatement: text(tableNames.haex.extension_migrations.columns.sqlStatement).notNull(),
  },
  (table) => [
    uniqueIndex('haex_extension_migrations_extension_id_migration_name_unique')
      .on(table.extensionId, table.migrationName),
  ],
)
export type InsertHaexExtensionMigrations = typeof haexExtensionMigrations.$inferInsert
export type SelectHaexExtensionMigrations = typeof haexExtensionMigrations.$inferSelect

// ---------------------------------------------------------------------------
// External Authorized & Blocked Clients
// ---------------------------------------------------------------------------

export const haexExternalAuthorizedClients = sqliteTable(
  tableNames.haex.external_authorized_clients.name,
  {
    id: text(tableNames.haex.external_authorized_clients.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    clientId: text(tableNames.haex.external_authorized_clients.columns.clientId).notNull(),
    clientName: text(tableNames.haex.external_authorized_clients.columns.clientName).notNull(),
    publicKey: text(tableNames.haex.external_authorized_clients.columns.publicKey).notNull(),
    extensionId: text(tableNames.haex.external_authorized_clients.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    authorizedAt: text(tableNames.haex.external_authorized_clients.columns.authorizedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    lastSeen: text(tableNames.haex.external_authorized_clients.columns.lastSeen),
  },
  (table) => [
    uniqueIndex('haex_external_authorized_clients_client_extension_unique')
      .on(table.clientId, table.extensionId),
  ],
)
export type InsertHaexExternalAuthorizedClients = typeof haexExternalAuthorizedClients.$inferInsert
export type SelectHaexExternalAuthorizedClients = typeof haexExternalAuthorizedClients.$inferSelect

export const haexExternalBlockedClients = sqliteTable(
  tableNames.haex.external_blocked_clients.name,
  {
    id: text(tableNames.haex.external_blocked_clients.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    clientId: text(tableNames.haex.external_blocked_clients.columns.clientId).notNull(),
    clientName: text(tableNames.haex.external_blocked_clients.columns.clientName).notNull(),
    publicKey: text(tableNames.haex.external_blocked_clients.columns.publicKey).notNull(),
    blockedAt: text(tableNames.haex.external_blocked_clients.columns.blockedAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
  },
  (table) => [
    uniqueIndex('haex_external_blocked_clients_client_id_unique').on(table.clientId),
  ],
)
export type InsertHaexExternalBlockedClients = typeof haexExternalBlockedClients.$inferInsert
export type SelectHaexExternalBlockedClients = typeof haexExternalBlockedClients.$inferSelect

// ---------------------------------------------------------------------------
// Extension Resource Limits
// ---------------------------------------------------------------------------

export const haexExtensionLimits = sqliteTable(
  tableNames.haex.extension_limits.name,
  {
    id: text(tableNames.haex.extension_limits.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    extensionId: text(tableNames.haex.extension_limits.columns.extensionId)
      .notNull()
      .references((): AnySQLiteColumn => haexExtensions.id, {
        onDelete: 'cascade',
      }),
    queryTimeoutMs: integer(tableNames.haex.extension_limits.columns.queryTimeoutMs)
      .notNull()
      .default(30000),
    maxResultRows: integer(tableNames.haex.extension_limits.columns.maxResultRows)
      .notNull()
      .default(10000),
    maxConcurrentQueries: integer(tableNames.haex.extension_limits.columns.maxConcurrentQueries)
      .notNull()
      .default(5),
    maxQuerySizeBytes: integer(tableNames.haex.extension_limits.columns.maxQuerySizeBytes)
      .notNull()
      .default(1048576),
    createdAt: text(tableNames.haex.extension_limits.columns.createdAt).default(sql`(CURRENT_TIMESTAMP)`),
    updatedAt: integer(tableNames.haex.extension_limits.columns.updatedAt, { mode: 'timestamp' })
      .$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_extension_limits_extension_id_unique').on(table.extensionId),
  ],
)
export type InsertHaexExtensionLimits = typeof haexExtensionLimits.$inferInsert
export type SelectHaexExtensionLimits = typeof haexExtensionLimits.$inferSelect
