/**
 * Table Scanner - Scans CRDT tables for changes to push/pull
 * Uses Tauri commands to interact with SQLite database
 */

import { invoke } from '@tauri-apps/api/core'
import type { ColumnInfo } from '@bindings/ColumnInfo'
import type { DirtyTable } from '@bindings/DirtyTable'
import { encryptCrdtData } from '@haex-space/vault-sdk'
import tableNames from '@/database/tableNames.json'
import { createLogger } from '@/stores/logging'

const CRDT_COLUMNS = tableNames.crdt.columns
const SYNC_METADATA_COLUMNS = tableNames.sync_metadata.columns

const log = createLogger('SYNC SCANNER')

export interface ColumnChange {
  tableName: string
  rowPks: string // JSON string of primary key values
  columnName: string
  hlcTimestamp: string
  batchId?: string // UUID identifying which changes belong together (optional for pull)
  batchSeq?: number // Sequence number within batch (optional for pull, 1-based)
  batchTotal?: number // Total number of changes in this batch (optional for pull)
  encryptedValue?: string
  nonce?: string
  deviceId: string // Device that created this change
}

/**
 * Gets schema information for a table
 */
export async function getTableSchemaAsync(
  tableName: string,
): Promise<ColumnInfo[]> {
  return await invoke('get_table_schema', { tableName })
}

/**
 * Gets all dirty tables that need to be synced
 */
export async function getDirtyTablesAsync(): Promise<DirtyTable[]> {
  return await invoke('get_dirty_tables')
}

/**
 * Gets all CRDT-enabled tables (tables with haex_tombstone column)
 */
export async function getAllCrdtTablesAsync(): Promise<string[]> {
  return await invoke('get_all_crdt_tables')
}

/**
 * Extracts primary key values from a row
 */
export function extractPrimaryKeys(
  row: Record<string, unknown>,
  pkColumns: ColumnInfo[],
): Record<string, unknown> {
  const pks: Record<string, unknown> = {}

  for (const pkCol of pkColumns) {
    pks[pkCol.name] = row[pkCol.name]
  }

  return pks
}

/**
 * Scans a table for rows that are newer than lastPushHlcTimestamp
 * Returns column-level changes for all modified columns
 * Note: batchSeq and batchTotal will be 0 and need to be set by the caller
 */
export async function scanTableForChangesAsync(
  tableName: string,
  lastPushHlcTimestamp: string | null,
  vaultKey: Uint8Array,
  batchId: string,
  deviceId: string,
): Promise<Omit<ColumnChange, 'batchSeq' | 'batchTotal'>[]> {
  log.info(`Scanning table: ${tableName}`)
  log.debug(`  lastPushHlcTimestamp: ${lastPushHlcTimestamp || '(none - full scan)'}`)

  // Get table schema
  const schema = await getTableSchemaAsync(tableName)
  const pkColumns = schema.filter((col) => col.isPk)
  const dataColumns = schema.filter(
    (col) =>
      !col.isPk &&
      col.name !== CRDT_COLUMNS.haexTimestamp &&
      col.name !== CRDT_COLUMNS.haexColumnHlcs &&
      col.name !== SYNC_METADATA_COLUMNS.lastPushHlcTimestamp &&
      col.name !== SYNC_METADATA_COLUMNS.lastPullServerTimestamp &&
      col.name !== SYNC_METADATA_COLUMNS.updatedAt &&
      col.name !== SYNC_METADATA_COLUMNS.createdAt,
  )
  // Note: haex_tombstone is included in dataColumns and will be synced like any other column

  log.debug(`  Schema: ${schema.length} columns, ${pkColumns.length} PKs, ${dataColumns.length} data columns`)

  if (pkColumns.length === 0) {
    log.error(`Table ${tableName} has no primary key`)
    throw new Error(`Table ${tableName} has no primary key`)
  }

  // Build SQL query with explicit column list (so we know the order)
  // Include PKs, data columns, haex_timestamp, and haex_column_hlcs
  const allColumns = [
    ...pkColumns.map((c) => c.name),
    ...dataColumns.map((c) => c.name),
    CRDT_COLUMNS.haexTimestamp,
    CRDT_COLUMNS.haexColumnHlcs,
  ]
  const columnList = allColumns.map((c) => `"${c}"`).join(', ')

  // Note: We scan ALL rows (including tombstoned ones) that are newer than lastPushHlcTimestamp
  const whereClause = lastPushHlcTimestamp
    ? `WHERE ${CRDT_COLUMNS.haexTimestamp} > ?`
    : ''
  const query = `SELECT ${columnList} FROM "${tableName}" ${whereClause}`
  const params = lastPushHlcTimestamp ? [lastPushHlcTimestamp] : []

  log.debug(`  SQL: ${query}`)
  log.debug(`  Params: ${JSON.stringify(params)}`)

  // Execute query using Tauri SQL command
  const result = await invoke<unknown[][]>('sql_select', {
    sql: query,
    params,
  })

  log.info(`  Query returned ${result.length} rows`)

  // Convert result to rows - we know the column order from allColumns
  const rows: Array<Record<string, unknown>> = []

  for (let i = 0; i < result.length; i++) {
    const row: Record<string, unknown> = {}
    const rowData = result[i]
    if (!rowData) continue

    for (let j = 0; j < allColumns.length; j++) {
      const colName = allColumns[j]
      if (colName) {
        row[colName] = rowData[j]
      }
    }
    rows.push(row)
  }

  if (rows.length > 0) {
    log.debug(`  First row PKs:`, extractPrimaryKeys(rows[0]!, pkColumns))
  }

  const changes: Omit<ColumnChange, 'batchSeq' | 'batchTotal'>[] = []

  for (const row of rows) {
    // Parse column HLCs from JSON
    const hlcsString = row[CRDT_COLUMNS.haexColumnHlcs]

    const columnHlcs: Record<string, string> = JSON.parse(
      typeof hlcsString === 'string' ? hlcsString : '{}',
    )

    // Extract primary keys
    const pks = extractPrimaryKeys(row, pkColumns)
    const pkJson = JSON.stringify(pks)

    // Get row-level HLC to use as fallback for columns without individual HLC
    const rowHlc = row[CRDT_COLUMNS.haexTimestamp] as string

    // For each data column, create a change entry if it has a newer HLC
    for (const col of dataColumns) {
      const columnHlc = columnHlcs[col.name]

      // Use row-level HLC as fallback if column doesn't have individual HLC yet
      // This ensures ALL columns (including NULL values) are pushed on first sync
      const hlcToUse = columnHlc || rowHlc

      if (!hlcToUse) {
        // This should never happen as every row must have haex_timestamp
        log.warn(`Column ${col.name} has no HLC and row has no haex_timestamp, skipping`)
        continue
      }

      // Check if this column's HLC is newer than lastPushHlcTimestamp
      if (
        !lastPushHlcTimestamp ||
        hlcToUse > lastPushHlcTimestamp
      ) {
        // Encrypt the column value (wrap in object for encryptCrdtDataAsync)
        const value = row[col.name]
        const valueObject = { value }
        const { encryptedData, nonce } = await encryptCrdtData(
          valueObject as object,
          vaultKey,
        )

        changes.push({
          tableName,
          rowPks: pkJson,
          columnName: col.name,
          hlcTimestamp: hlcToUse,
          batchId,
          deviceId,
          encryptedValue: encryptedData,
          nonce,
        })
      }
    }
  }

  log.info(`  Generated ${changes.length} column changes from ${rows.length} rows`)

  return changes
}

/**
 * Clears a table from the dirty tables tracker.
 * If beforeTimestamp is provided, only clears entries with last_modified <= that timestamp.
 * This prevents clearing entries that were added AFTER the sync scan started.
 */
export async function clearDirtyTableAsync(
  tableName: string,
  beforeTimestamp?: string,
): Promise<void> {
  await invoke('clear_dirty_table', { tableName, beforeTimestamp })
}

/**
 * Clears all dirty tables
 */
export async function clearAllDirtyTablesAsync(): Promise<void> {
  await invoke('clear_all_dirty_tables')
}
