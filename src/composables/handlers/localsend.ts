// Handler for LocalSend extension methods (iframe mode)
// Maps SDK LocalSendAPI methods to Tauri commands

import { invoke } from '@tauri-apps/api/core'
import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'

/**
 * Handle LocalSend method calls from iframe extensions
 */
export async function handleLocalSendMethodAsync(
  request: ExtensionRequest,
  _extension: IHaexSpaceExtension,
) {
  if (!request) return

  switch (request.method) {
    // ========================================================================
    // Initialization
    // ========================================================================
    case TAURI_COMMANDS.localsend.init: {
      return invoke(TAURI_COMMANDS.localsend.init, {})
    }

    case TAURI_COMMANDS.localsend.getDeviceInfo: {
      return invoke(TAURI_COMMANDS.localsend.getDeviceInfo, {})
    }

    case TAURI_COMMANDS.localsend.setAlias: {
      const params = request.params as { alias: string }
      return invoke(TAURI_COMMANDS.localsend.setAlias, { alias: params.alias })
    }

    // ========================================================================
    // Settings
    // ========================================================================
    case TAURI_COMMANDS.localsend.getSettings: {
      return invoke(TAURI_COMMANDS.localsend.getSettings, {})
    }

    case TAURI_COMMANDS.localsend.setSettings: {
      const params = request.params as { settings: unknown }
      return invoke(TAURI_COMMANDS.localsend.setSettings, { settings: params.settings })
    }

    // ========================================================================
    // Discovery (Desktop only)
    // ========================================================================
    case TAURI_COMMANDS.localsend.startDiscovery: {
      return invoke(TAURI_COMMANDS.localsend.startDiscovery, {})
    }

    case TAURI_COMMANDS.localsend.stopDiscovery: {
      return invoke(TAURI_COMMANDS.localsend.stopDiscovery, {})
    }

    case TAURI_COMMANDS.localsend.getDevices: {
      return invoke(TAURI_COMMANDS.localsend.getDevices, {})
    }

    // ========================================================================
    // Network Scan (Mobile only)
    // ========================================================================
    case TAURI_COMMANDS.localsend.scanNetwork: {
      return invoke(TAURI_COMMANDS.localsend.scanNetwork, {})
    }

    // ========================================================================
    // Server (Receiving files)
    // ========================================================================
    case TAURI_COMMANDS.localsend.startServer: {
      const params = request.params as { port?: number } | undefined
      return invoke(TAURI_COMMANDS.localsend.startServer, { port: params?.port })
    }

    case TAURI_COMMANDS.localsend.stopServer: {
      return invoke(TAURI_COMMANDS.localsend.stopServer, {})
    }

    case TAURI_COMMANDS.localsend.getServerStatus: {
      return invoke(TAURI_COMMANDS.localsend.getServerStatus, {})
    }

    case TAURI_COMMANDS.localsend.getPendingTransfers: {
      return invoke(TAURI_COMMANDS.localsend.getPendingTransfers, {})
    }

    case TAURI_COMMANDS.localsend.acceptTransfer: {
      const params = request.params as { sessionId: string, saveDir: string }
      return invoke(TAURI_COMMANDS.localsend.acceptTransfer, {
        sessionId: params.sessionId,
        saveDir: params.saveDir,
      })
    }

    case TAURI_COMMANDS.localsend.rejectTransfer: {
      const params = request.params as { sessionId: string }
      return invoke(TAURI_COMMANDS.localsend.rejectTransfer, {
        sessionId: params.sessionId,
      })
    }

    // ========================================================================
    // Client (Sending files)
    // ========================================================================
    case TAURI_COMMANDS.localsend.prepareFiles: {
      const params = request.params as { paths: string[] }
      return invoke(TAURI_COMMANDS.localsend.prepareFiles, { paths: params.paths })
    }

    case TAURI_COMMANDS.localsend.sendFiles: {
      const params = request.params as { device: unknown, files: unknown[] }
      return invoke(TAURI_COMMANDS.localsend.sendFiles, {
        device: params.device,
        files: params.files,
      })
    }

    case TAURI_COMMANDS.localsend.cancelSend: {
      const params = request.params as { sessionId: string }
      return invoke(TAURI_COMMANDS.localsend.cancelSend, {
        sessionId: params.sessionId,
      })
    }

    default:
      throw new Error(`Unknown LocalSend method: ${request.method}`)
  }
}
