/**
 * Table Scanner - Scans CRDT tables for changes to push/pull
 * Uses Tauri commands to interact with SQLite database
 */

import { invoke } from '@tauri-apps/api/core'
import type { ColumnInfo } from '@bindings/ColumnInfo'
import type { DirtyTable } from '@bindings/DirtyTable'
import { encryptCrdtDataAsync } from '~/utils/crypto/vaultKey'
import tableNames from '@/database/tableNames.json'

const CRDT_COLUMNS = tableNames.crdt.columns

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
  // Get table schema
  const schema = await getTableSchemaAsync(tableName)
  const pkColumns = schema.filter((col) => col.isPk)
  const dataColumns = schema.filter(
    (col) =>
      !col.isPk &&
      col.name !== CRDT_COLUMNS.haexTimestamp &&
      col.name !== CRDT_COLUMNS.haexColumnHlcs,
  )
  // Note: haex_tombstone is included in dataColumns and will be synced like any other column

  if (pkColumns.length === 0) {
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

  // Execute query using Tauri SQL command
  const result = await invoke<unknown[][]>('sql_select', {
    sql: query,
    params,
  })

  console.log(
    `Table ${tableName}: SQL result structure:`,
    'result.length:',
    result.length,
    'allColumns:',
    allColumns,
  )

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

  console.log(
    `Table ${tableName}: Found ${rows.length} rows matching WHERE clause`,
  )
  console.log(`Table ${tableName}: First row:`, rows[0])

  const changes: Omit<ColumnChange, 'batchSeq' | 'batchTotal'>[] = []

  for (const row of rows) {
    // Parse column HLCs from JSON
    const hlcsString = row[CRDT_COLUMNS.haexColumnHlcs]
    console.log(
      `Table ${tableName}: Raw hlcsString:`,
      hlcsString,
      'type:',
      typeof hlcsString,
      'CRDT_COLUMNS.haexColumnHlcs:',
      CRDT_COLUMNS.haexColumnHlcs,
    )
    console.log(`Table ${tableName}: All row keys:`, Object.keys(row))

    const columnHlcs: Record<string, string> = JSON.parse(
      typeof hlcsString === 'string' ? hlcsString : '{}',
    )

    console.log(
      `Table ${tableName}: Row columnHlcs:`,
      columnHlcs,
      'dataColumns:',
      dataColumns.map((c) => c.name),
    )

    // Extract primary keys
    const pks = extractPrimaryKeys(row, pkColumns)
    const pkJson = JSON.stringify(pks)

    // For each data column, create a change entry if it has a newer HLC
    for (const col of dataColumns) {
      const columnHlc = columnHlcs[col.name]

      if (!columnHlc) {
        // Column doesn't have HLC yet - skip
        console.log(
          `Table ${tableName}: Column ${col.name} has no HLC, skipping`,
        )
        continue
      }

      // Check if this column's HLC is newer than lastPushHlcTimestamp
      if (
        !lastPushHlcTimestamp ||
        columnHlc > lastPushHlcTimestamp
      ) {
        // Encrypt the column value (wrap in object for encryptCrdtDataAsync)
        const value = row[col.name]
        const valueObject = { value }
        const { encryptedData, nonce } = await encryptCrdtDataAsync(
          valueObject as object,
          vaultKey,
        )

        changes.push({
          tableName,
          rowPks: pkJson,
          columnName: col.name,
          hlcTimestamp: columnHlc,
          batchId,
          deviceId,
          encryptedValue: encryptedData,
          nonce,
        })
      }
    }
  }

  return changes
}

/**
 * Clears a table from the dirty tables tracker
 */
export async function clearDirtyTableAsync(
  tableName: string,
): Promise<void> {
  await invoke('clear_dirty_table', { tableName })
}

/**
 * Clears all dirty tables
 */
export async function clearAllDirtyTablesAsync(): Promise<void> {
  await invoke('clear_all_dirty_tables')
}
