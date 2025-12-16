// Handler for FileSync extension methods
// Maps SDK FileSyncAPI methods to Tauri commands

import { invoke } from '@tauri-apps/api/core'
import { HAEXTENSION_METHODS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'

export async function handleFileSyncMethodAsync(
  request: ExtensionRequest,
  _extension: IHaexSpaceExtension,
) {
  if (!request) return

  switch (request.method) {
    // ========================================================================
    // Spaces
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listSpaces: {
      return invoke('filesync_list_spaces')
    }

    case HAEXTENSION_METHODS.filesystem.sync.createSpace: {
      return invoke('filesync_create_space', { request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.deleteSpace: {
      const params = request.params as { spaceId: string }
      return invoke('filesync_delete_space', { spaceId: params.spaceId })
    }

    // ========================================================================
    // Files
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listFiles: {
      return invoke('filesync_list_files', { request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.getFile: {
      const params = request.params as { fileId: string }
      return invoke('filesync_get_file', { fileId: params.fileId })
    }

    case HAEXTENSION_METHODS.filesystem.sync.uploadFile: {
      return invoke('filesync_upload_file', { request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.downloadFile: {
      return invoke('filesync_download_file', { request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.deleteFile: {
      const params = request.params as { fileId: string }
      return invoke('filesync_delete_file', { fileId: params.fileId })
    }

    // ========================================================================
    // Backends
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listBackends: {
      return invoke('filesync_list_backends')
    }

    case HAEXTENSION_METHODS.filesystem.sync.addBackend: {
      return invoke('filesync_add_backend', { request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.removeBackend: {
      const params = request.params as { backendId: string }
      return invoke('filesync_remove_backend', { backendId: params.backendId })
    }

    case HAEXTENSION_METHODS.filesystem.sync.testBackend: {
      const params = request.params as { backendId: string }
      return invoke('filesync_test_backend', { backendId: params.backendId })
    }

    // ========================================================================
    // Sync Rules
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.listSyncRules: {
      return invoke('filesync_list_sync_rules')
    }

    case HAEXTENSION_METHODS.filesystem.sync.addSyncRule: {
      return invoke('filesync_add_sync_rule', { request: request.params })
    }

    case HAEXTENSION_METHODS.filesystem.sync.removeSyncRule: {
      const params = request.params as { ruleId: string }
      return invoke('filesync_remove_sync_rule', { ruleId: params.ruleId })
    }

    // ========================================================================
    // Sync Operations
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.getSyncStatus: {
      return invoke('filesync_get_sync_status')
    }

    case HAEXTENSION_METHODS.filesystem.sync.triggerSync: {
      return invoke('filesync_trigger_sync')
    }

    case HAEXTENSION_METHODS.filesystem.sync.pauseSync: {
      return invoke('filesync_pause_sync')
    }

    case HAEXTENSION_METHODS.filesystem.sync.resumeSync: {
      return invoke('filesync_resume_sync')
    }

    // ========================================================================
    // Conflict Resolution
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.resolveConflict: {
      return invoke('filesync_resolve_conflict', { request: request.params })
    }

    // ========================================================================
    // UI Helpers
    // ========================================================================
    case HAEXTENSION_METHODS.filesystem.sync.selectFolder: {
      return invoke('filesync_select_folder')
    }

    default:
      throw new Error(`Unknown filesync method: ${request.method}`)
  }
}
