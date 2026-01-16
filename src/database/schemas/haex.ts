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

// Note: CRDT columns (haex_timestamp, haex_column_hlcs, haex_tombstone) are added
// automatically by the Rust CrdtTransformer when CREATE TABLE is executed.
// The WHERE haex_tombstone = 0 condition for UNIQUE indices is also added automatically.

export const haexDevices = sqliteTable(
  tableNames.haex.devices.name,
  {
    id: text(tableNames.haex.devices.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    deviceId: text(tableNames.haex.devices.columns.deviceId).notNull(),
    name: text(tableNames.haex.devices.columns.name).notNull(),
    current: integer(tableNames.haex.devices.columns.current, {
      mode: 'boolean',
    })
      .default(false)
      .notNull(),
    createdAt: text(tableNames.haex.devices.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.devices.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_devices_device_id_unique').on(table.deviceId),
  ],
)
export type InsertHaexDevices = typeof haexDevices.$inferInsert
export type SelectHaexDevices = typeof haexDevices.$inferSelect

export const haexVaultSettings = sqliteTable(
  tableNames.haex.vault_settings.name,
  {
    id: text(tableNames.haex.vault_settings.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    key: text(tableNames.haex.vault_settings.columns.key).notNull(),
    type: text(tableNames.haex.vault_settings.columns.type).notNull(),
    value: text(tableNames.haex.vault_settings.columns.value),
  },
  (table) => [
    uniqueIndex('haex_vault_settings_key_type_unique').on(table.key, table.type),
  ],
)
export type InsertHaexVaultSettings = typeof haexVaultSettings.$inferInsert
export type SelectHaexVaultSettings = typeof haexVaultSettings.$inferSelect

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
    // Für Extensions (wenn itemType = 'extension')
    extensionId: text(
      tableNames.haex.desktop_items.columns.extensionId,
    ).references((): AnySQLiteColumn => haexExtensions.id, {
      onDelete: 'cascade',
    }),
    // Für System Windows (wenn itemType = 'system')
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

export const haexSyncBackends = sqliteTable(
  tableNames.haex.sync_backends.name,
  {
    id: text(tableNames.haex.sync_backends.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.sync_backends.columns.name).notNull(),
    serverUrl: text(tableNames.haex.sync_backends.columns.serverUrl).notNull(),
    vaultId: text(tableNames.haex.sync_backends.columns.vaultId),
    email: text(tableNames.haex.sync_backends.columns.email),
    password: text(tableNames.haex.sync_backends.columns.password),
    syncKey: text(tableNames.haex.sync_backends.columns.syncKey),
    vaultKeySalt: text(tableNames.haex.sync_backends.columns.vaultKeySalt),
    enabled: integer(tableNames.haex.sync_backends.columns.enabled, {
      mode: 'boolean',
    })
      .default(true)
      .notNull(),
    priority: integer(tableNames.haex.sync_backends.columns.priority)
      .default(0)
      .notNull(),
    lastPushHlcTimestamp: text(tableNames.haex.sync_backends.columns.lastPushHlcTimestamp),
    lastPullServerTimestamp: text(tableNames.haex.sync_backends.columns.lastPullServerTimestamp),
    pendingVaultKeyUpdate: integer(tableNames.haex.sync_backends.columns.pendingVaultKeyUpdate, {
      mode: 'boolean',
    })
      .default(false)
      .notNull(),
    createdAt: text(tableNames.haex.sync_backends.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.sync_backends.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  },
  (table) => [
    uniqueIndex('haex_sync_backends_server_url_email_unique').on(table.serverUrl, table.email),
  ],
)
export type InsertHaexSyncBackends = typeof haexSyncBackends.$inferInsert
export type SelectHaexSyncBackends = typeof haexSyncBackends.$inferSelect

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

// External authorized clients (browser extensions, CLI tools, servers, etc.)
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

// External blocked clients (permanently denied)
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

// Extension resource limits (query timeouts, result size limits, etc.)
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
    // Query timeout in milliseconds (default: 30000 = 30 seconds)
    queryTimeoutMs: integer(tableNames.haex.extension_limits.columns.queryTimeoutMs)
      .notNull()
      .default(30000),
    // Maximum number of rows returned per query (default: 10000)
    maxResultRows: integer(tableNames.haex.extension_limits.columns.maxResultRows)
      .notNull()
      .default(10000),
    // Maximum concurrent queries per extension (default: 5)
    maxConcurrentQueries: integer(tableNames.haex.extension_limits.columns.maxConcurrentQueries)
      .notNull()
      .default(5),
    // Maximum query SQL size in bytes (default: 1MB)
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
