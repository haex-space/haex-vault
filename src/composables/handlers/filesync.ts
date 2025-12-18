// Handler for FileSync extension methods
// Maps SDK FileSyncAPI methods to Tauri commands

import { invoke } from '@tauri-apps/api/core'
import { HAEXTENSION_METHODS, TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'

export async function handleFileSyncMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!request) return

  // Extension identification for Rust commands (secure - set by host app)
  const extInfo = {
    publicKey: extension.publicKey,
    name: extension.name,
  }

  switch (request.method) {
    // ========================================================================
    // Spaces
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listSpaces: {
      return invoke(TAURI_COMMANDS.filesync.listSpaces, extInfo)
    }

    case HAEXTENSION_METHODS.filesystem.sync.createSpace: {
      return invoke(TAURI_COMMANDS.filesync.createSpace, { ...extInfo, request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.deleteSpace: {
      const params = request.params as { spaceId: string }
      return invoke(TAURI_COMMANDS.filesync.deleteSpace, { ...extInfo, spaceId: params.spaceId })
    }

    // ========================================================================
    // Files
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listFiles: {
      return invoke(TAURI_COMMANDS.filesync.listFiles, { ...extInfo, request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.getFile: {
      const params = request.params as { fileId: string }
      return invoke(TAURI_COMMANDS.filesync.getFile, { ...extInfo, fileId: params.fileId })
    }

    case HAEXTENSION_METHODS.filesystem.sync.uploadFile: {
      return invoke(TAURI_COMMANDS.filesync.uploadFile, { ...extInfo, request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.downloadFile: {
      return invoke(TAURI_COMMANDS.filesync.downloadFile, { ...extInfo, request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.deleteFile: {
      const params = request.params as { fileId: string }
      return invoke(TAURI_COMMANDS.filesync.deleteFile, { ...extInfo, fileId: params.fileId })
    }

    // ========================================================================
    // Backends
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listBackends: {
      return invoke(TAURI_COMMANDS.filesync.listBackends, extInfo)
    }

    case HAEXTENSION_METHODS.filesystem.sync.addBackend: {
      return invoke(TAURI_COMMANDS.filesync.addBackend, { ...extInfo, request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.removeBackend: {
      const params = request.params as { backendId: string }
      return invoke(TAURI_COMMANDS.filesync.removeBackend, { ...extInfo, backendId: params.backendId })
    }

    case HAEXTENSION_METHODS.filesystem.sync.testBackend: {
      const params = request.params as { backendId: string }
      return invoke(TAURI_COMMANDS.filesync.testBackend, { ...extInfo, backendId: params.backendId })
    }

    // ========================================================================
    // Sync Rules
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listSyncRules: {
      return invoke(TAURI_COMMANDS.filesync.listSyncRules, extInfo)
    }

    case HAEXTENSION_METHODS.filesystem.sync.addSyncRule: {
      return invoke(TAURI_COMMANDS.filesync.addSyncRule, { ...extInfo, request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.removeSyncRule: {
      const params = request.params as { ruleId: string }
      return invoke(TAURI_COMMANDS.filesync.removeSyncRule, { ...extInfo, ruleId: params.ruleId })
    }

    // ========================================================================
    // Sync Operations
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.getSyncStatus: {
      return invoke(TAURI_COMMANDS.filesync.getSyncStatus, extInfo)
    }

    case HAEXTENSION_METHODS.filesystem.sync.triggerSync: {
      return invoke(TAURI_COMMANDS.filesync.triggerSync, extInfo)
    }

    case HAEXTENSION_METHODS.filesystem.sync.pauseSync: {
      return invoke(TAURI_COMMANDS.filesync.pauseSync, extInfo)
    }

    case HAEXTENSION_METHODS.filesystem.sync.resumeSync: {
      return invoke(TAURI_COMMANDS.filesync.resumeSync, extInfo)
    }

    // ========================================================================
    // Conflict Resolution
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.resolveConflict: {
      return invoke(TAURI_COMMANDS.filesync.resolveConflict, { ...extInfo, request: request.params })
    }

    // ========================================================================
    // UI Helpers
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.selectFolder: {
      // selectFolder doesn't need extension info - just opens a dialog
      return invoke(TAURI_COMMANDS.filesync.selectFolder)
    }

    case HAEXTENSION_METHODS.filesystem.sync.scanLocal: {
      return invoke(TAURI_COMMANDS.filesync.scanLocal, { ...extInfo, request: request.params })
    }

    default:
      throw new Error(`Unknown filesync method: ${request.method}`)
  }
}
