import { invoke } from '@tauri-apps/api/core'
import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

export async function handleDatabaseMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  const params = request.params as {
    query?: string
    params?: unknown[]
  }

  switch (request.method) {
    case TAURI_COMMANDS.database.query: {
      try {
        const rows = await invokeWithPermissionPrompt<unknown[]>('extension_sql_select', {
          sql: params.query || '',
          params: params.params || [],
          publicKey: extension.publicKey,
          name: extension.name,
        })

        return {
          rows,
          rowsAffected: 0,
          lastInsertId: undefined,
        }
      } catch (error) {
        // If error is about non-SELECT statements (INSERT/UPDATE/DELETE with RETURNING),
        // automatically retry with execute
        const errorMessage = error instanceof Error ? error.message : String(error)
        if (errorMessage.includes('Only SELECT statements are allowed')) {
          const rows = await invokeWithPermissionPrompt<unknown[]>('extension_sql_execute', {
            sql: params.query || '',
            params: params.params || [],
            publicKey: extension.publicKey,
            name: extension.name,
          })

          return {
            rows,
            rowsAffected: rows.length,
            lastInsertId: undefined,
          }
        }
        throw error
      }
    }

    case TAURI_COMMANDS.database.execute: {
      const rows = await invokeWithPermissionPrompt<unknown[]>('extension_sql_execute', {
        sql: params.query || '',
        params: params.params || [],
        publicKey: extension.publicKey,
        name: extension.name,
      })

      return {
        rows,
        rowsAffected: 1,
        lastInsertId: undefined,
      }
    }

    case TAURI_COMMANDS.database.transaction: {
      const statements =
        (request.params as { statements?: string[] }).statements || []

      for (const stmt of statements) {
        await invokeWithPermissionPrompt('extension_sql_execute', {
          sql: stmt,
          params: [],
          publicKey: extension.publicKey,
          name: extension.name,
        })
      }

      return { success: true }
    }

    case TAURI_COMMANDS.database.registerMigrations: {
      const migrationParams = request.params as {
        extensionVersion: string
        migrations: Array<{ name: string; sql: string }>
      }

      const result = await invoke<{
        appliedCount: number
        alreadyAppliedCount: number
        appliedMigrations: string[]
      }>('register_extension_migrations', {
        publicKey: extension.publicKey,
        extensionName: extension.name,
        extensionVersion: migrationParams.extensionVersion,
        migrations: migrationParams.migrations,
      })

      return result
    }

    default:
      throw new Error(`Unknown database method: ${request.method}`)
  }
}
