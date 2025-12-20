// Handler for Remote Storage extension methods
// Maps SDK RemoteStorageAPI methods to Tauri commands with permission checks

import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

export async function handleRemoteStorageMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!request) return

  switch (request.method) {
    // ========================================================================
    // Backend Management
    // ========================================================================
    case TAURI_COMMANDS.remoteStorage.listBackends: {
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.listBackends, {
        publicKey: extension.publicKey,
        name: extension.name,
      })
    }

    case TAURI_COMMANDS.remoteStorage.addBackend: {
      const params = request.params as {
        name: string
        type: string
        config: Record<string, unknown>
      }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.addBackend, {
        publicKey: extension.publicKey,
        name: extension.name,
        request: {
          name: params.name,
          type: params.type,
          config: params.config,
        },
      })
    }

    case TAURI_COMMANDS.remoteStorage.updateBackend: {
      const params = request.params as {
        backendId: string
        name?: string
        config?: Record<string, unknown>
      }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.updateBackend, {
        publicKey: extension.publicKey,
        name: extension.name,
        request: params,
      })
    }

    case TAURI_COMMANDS.remoteStorage.removeBackend: {
      const params = request.params as { backendId: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.removeBackend, {
        publicKey: extension.publicKey,
        name: extension.name,
        backendId: params.backendId,
      })
    }

    case TAURI_COMMANDS.remoteStorage.testBackend: {
      const params = request.params as { backendId: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.testBackend, {
        publicKey: extension.publicKey,
        name: extension.name,
        backendId: params.backendId,
      })
    }

    // ========================================================================
    // Storage Operations
    // ========================================================================
    case TAURI_COMMANDS.remoteStorage.upload: {
      const params = request.params as {
        backendId: string
        key: string
        data: string
      }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.upload, {
        publicKey: extension.publicKey,
        name: extension.name,
        request: {
          backendId: params.backendId,
          key: params.key,
          data: params.data,
        },
      })
    }

    case TAURI_COMMANDS.remoteStorage.download: {
      const params = request.params as {
        backendId: string
        key: string
      }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.download, {
        publicKey: extension.publicKey,
        name: extension.name,
        request: {
          backendId: params.backendId,
          key: params.key,
        },
      })
    }

    case TAURI_COMMANDS.remoteStorage.delete: {
      const params = request.params as {
        backendId: string
        key: string
      }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.delete, {
        publicKey: extension.publicKey,
        name: extension.name,
        request: {
          backendId: params.backendId,
          key: params.key,
        },
      })
    }

    case TAURI_COMMANDS.remoteStorage.list: {
      const params = request.params as {
        backendId: string
        prefix?: string
      }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.remoteStorage.list, {
        publicKey: extension.publicKey,
        name: extension.name,
        request: {
          backendId: params.backendId,
          prefix: params.prefix,
        },
      })
    }

    default:
      throw new Error(`Unknown remote storage method: ${request.method}`)
  }
}
