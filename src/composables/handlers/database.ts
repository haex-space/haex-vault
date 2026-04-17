import { invoke } from '@tauri-apps/api/core'
import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'
import { useExtensionReadyStore } from '~/stores/extensions/ready'

interface DatabaseQueryResult {
  rows: unknown[]
  rowsAffected: number
  lastInsertId?: number
}

export async function handleDatabaseMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  const params = request.params as {
    sql?: string
    params?: unknown[]
  }

  switch (request.method) {
    case TAURI_COMMANDS.database.query: {
      try {
        // Backend now returns DatabaseQueryResult directly
        const result = await invokeWithPermissionPrompt<DatabaseQueryResult>(TAURI_COMMANDS.database.query, {
          sql: params.sql || '',
          params: params.params || [],
          publicKey: extension.publicKey,
          name: extension.name,
        })
        return result
      } catch (error) {
        // If error is about non-SELECT statements (INSERT/UPDATE/DELETE with RETURNING),
        // automatically retry with execute
        const errorMessage = error instanceof Error ? error.message : String(error)
        if (errorMessage.includes('Only SELECT statements are allowed')) {
          const result = await invokeWithPermissionPrompt<DatabaseQueryResult>(TAURI_COMMANDS.database.execute, {
            sql: params.sql || '',
            params: params.params || [],
            publicKey: extension.publicKey,
            name: extension.name,
          })
          return result
        }
        throw error
      }
    }

    case TAURI_COMMANDS.database.execute: {
      // Backend now returns DatabaseQueryResult directly
      const result = await invokeWithPermissionPrompt<DatabaseQueryResult>(TAURI_COMMANDS.database.execute, {
        sql: params.sql || '',
        params: params.params || [],
        publicKey: extension.publicKey,
        name: extension.name,
      })
      return result
    }

    case TAURI_COMMANDS.database.transaction: {
      const transactionParams = request.params as {
        statements?: Array<{ sql: string; params?: unknown[] }>
      }
      const statementPairs = (transactionParams.statements || []).map(
        s => [s.sql, s.params || []] as [string, unknown[]]
      )

      return invokeWithPermissionPrompt('extension_database_transaction', {
        statements: statementPairs,
        publicKey: extension.publicKey,
        name: extension.name,
      })
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
      }>(TAURI_COMMANDS.database.registerMigrations, {
        publicKey: extension.publicKey,
        name: extension.name,
        extensionVersion: migrationParams.extensionVersion,
        migrations: migrationParams.migrations,
      })

      // Signal that the extension is ready after successful migration registration
      // This unblocks:
      // - ExternalBridge waiting for extension to handle requests (Desktop)
      // - Other extensions waiting for this extension to be ready (all platforms)
      const extensionReadyStore = useExtensionReadyStore()
      await extensionReadyStore.signalReady(extension.id)

      return result
    }

    default:
      throw new Error(`Unknown database method: ${request.method}`)
  }
}
