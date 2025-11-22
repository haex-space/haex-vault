import { sql } from 'drizzle-orm'
import {
  check,
  integer,
  sqliteTable,
  text,
  uniqueIndex,
  type AnySQLiteColumn,
  type SQLiteColumnBuilderBase,
} from 'drizzle-orm/sqlite-core'
import tableNames from '@/database/tableNames.json'

const crdtColumnNames = tableNames.crdt.columns

// Helper function to add common CRDT columns (haexTimestamp, haexColumnHlcs, haexTombstone)
export const withCrdtColumns = <
  T extends Record<string, SQLiteColumnBuilderBase>,
>(
  columns: T,
) => ({
  ...columns,
  haexTimestamp: text(crdtColumnNames.haexTimestamp),
  haexColumnHlcs: text(crdtColumnNames.haexColumnHlcs).notNull().default('{}'),
  haexTombstone: integer(crdtColumnNames.haexTombstone, { mode: 'boolean' })
    .notNull()
    .default(false),
})

export const haexDevices = sqliteTable(
  tableNames.haex.devices.name,
  withCrdtColumns({
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
  }),
  (table) => [
    uniqueIndex('haex_devices_device_id_unique')
      .on(table.deviceId)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexDevices = typeof haexDevices.$inferInsert
export type SelectHaexDevices = typeof haexDevices.$inferSelect

export const haexSettings = sqliteTable(
  tableNames.haex.settings.name,
  withCrdtColumns({
    id: text(tableNames.haex.settings.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    deviceId: text(tableNames.haex.settings.columns.deviceId).references(
      (): AnySQLiteColumn => haexDevices.id,
      { onDelete: 'cascade' },
    ),
    key: text(tableNames.haex.settings.columns.key),
    type: text(tableNames.haex.settings.columns.type),
    value: text(tableNames.haex.settings.columns.value),
  }),
  (table) => [
    uniqueIndex('haex_settings_device_id_key_type_unique')
      .on(table.deviceId, table.key, table.type)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexSettings = typeof haexSettings.$inferInsert
export type SelectHaexSettings = typeof haexSettings.$inferSelect

export const haexExtensions = sqliteTable(
  tableNames.haex.extensions.name,
  withCrdtColumns({
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
    createdAt: text(tableNames.haex.extensions.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.extensions.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  }),
  (table) => [
    // UNIQUE constraint: Pro Developer (public_key) kann nur eine Extension mit diesem Namen existieren
    uniqueIndex('haex_extensions_public_key_name_unique')
      .on(table.public_key, table.name)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexExtensions = typeof haexExtensions.$inferInsert
export type SelectHaexExtensions = typeof haexExtensions.$inferSelect

export const haexExtensionPermissions = sqliteTable(
  tableNames.haex.extension_permissions.name,
  withCrdtColumns({
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
  }),
  (table) => [
    uniqueIndex('haex_extension_permissions_extension_id_resource_type_action_target_unique')
      .on(table.extensionId, table.resourceType, table.action, table.target)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InserthaexExtensionPermissions =
  typeof haexExtensionPermissions.$inferInsert
export type SelecthaexExtensionPermissions =
  typeof haexExtensionPermissions.$inferSelect

export const haexNotifications = sqliteTable(
  tableNames.haex.notifications.name,
  withCrdtColumns({
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
  }),
)
export type InsertHaexNotifications = typeof haexNotifications.$inferInsert
export type SelectHaexNotifications = typeof haexNotifications.$inferSelect

export const haexWorkspaces = sqliteTable(
  tableNames.haex.workspaces.name,
  withCrdtColumns({
    id: text(tableNames.haex.workspaces.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    deviceId: text(tableNames.haex.workspaces.columns.deviceId).notNull(),
    name: text(tableNames.haex.workspaces.columns.name).notNull(),
    position: integer(tableNames.haex.workspaces.columns.position)
      .notNull()
      .default(0),
    background: text(),
  }),
  (table) => [
    uniqueIndex('haex_workspaces_position_unique')
      .on(table.position)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexWorkspaces = typeof haexWorkspaces.$inferInsert
export type SelectHaexWorkspaces = typeof haexWorkspaces.$inferSelect

export const haexDesktopItems = sqliteTable(
  tableNames.haex.desktop_items.name,
  withCrdtColumns({
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
  }),
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
  withCrdtColumns({
    id: text(tableNames.haex.sync_backends.columns.id)
      .$defaultFn(() => crypto.randomUUID())
      .primaryKey(),
    name: text(tableNames.haex.sync_backends.columns.name).notNull(),
    serverUrl: text(tableNames.haex.sync_backends.columns.serverUrl).notNull(),
    vaultId: text(tableNames.haex.sync_backends.columns.vaultId),
    email: text(tableNames.haex.sync_backends.columns.email),
    password: text(tableNames.haex.sync_backends.columns.password),
    syncKey: text(tableNames.haex.sync_backends.columns.syncKey),
    enabled: integer(tableNames.haex.sync_backends.columns.enabled, {
      mode: 'boolean',
    })
      .default(true)
      .notNull(),
    priority: integer(tableNames.haex.sync_backends.columns.priority)
      .default(0)
      .notNull(),
    lastPushHlcTimestamp: text(tableNames.haex.sync_backends.columns.lastPushHlcTimestamp),
    lastPullHlcTimestamp: text(tableNames.haex.sync_backends.columns.lastPullHlcTimestamp),
    createdAt: text(tableNames.haex.sync_backends.columns.createdAt).default(
      sql`(CURRENT_TIMESTAMP)`,
    ),
    updatedAt: integer(tableNames.haex.sync_backends.columns.updatedAt, {
      mode: 'timestamp',
    }).$onUpdate(() => new Date()),
  }),
  (table) => [
    uniqueIndex('haex_sync_backends_server_url_email_unique')
      .on(table.serverUrl, table.email)
      .where(sql`${table.haexTombstone} = 0`),
  ],
)
export type InsertHaexSyncBackends = typeof haexSyncBackends.$inferInsert
export type SelectHaexSyncBackends = typeof haexSyncBackends.$inferSelect
