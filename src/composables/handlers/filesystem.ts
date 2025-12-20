import { save } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import { openPath } from '@tauri-apps/plugin-opener'
import { tempDir, join } from '@tauri-apps/api/path'
import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

export async function handleFilesystemMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!request || !extension) return

  switch (request.method) {
    case TAURI_COMMANDS.filesystem.saveFile: {
      const params = request.params as {
        data: number[]
        defaultPath?: string
        title?: string
        filters?: Array<{ name: string; extensions: string[] }>
      }

      // Convert number array back to Uint8Array
      const data = new Uint8Array(params.data)

      // Open save dialog
      const filePath = await save({
        defaultPath: params.defaultPath,
        title: params.title || 'Save File',
        filters: params.filters,
      })

      // User cancelled
      if (!filePath) {
        return null
      }

      // Write file
      await writeFile(filePath, data)

      return {
        path: filePath,
        success: true,
      }
    }

    case TAURI_COMMANDS.filesystem.openFile: {
      const params = request.params as {
        data: number[]
        fileName: string
        mimeType?: string
      }

      try {
        // Convert number array back to Uint8Array
        const data = new Uint8Array(params.data)

        // Get temp directory and create file path
        const tempDirPath = await tempDir()
        const tempFilePath = await join(tempDirPath, params.fileName)

        // Write file to temp directory
        await writeFile(tempFilePath, data)

        // Open file with system's default viewer
        await openPath(tempFilePath)

        return {
          success: true,
        }
      } catch (error) {
        console.error('[Filesystem] Error opening file:', error)
        return {
          success: false,
        }
      }
    }

    // ========================================================================
    // Generic Filesystem Operations (with permission checks)
    // ========================================================================

    case TAURI_COMMANDS.filesystem.readFile: {
      const params = request.params as { path: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.readFile, {
        publicKey: extension.publicKey,
        name: extension.name,
        path: params.path,
      })
    }

    case TAURI_COMMANDS.filesystem.writeFile: {
      const params = request.params as { path: string; data: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.writeFile, {
        publicKey: extension.publicKey,
        name: extension.name,
        path: params.path,
        data: params.data,
      })
    }

    case TAURI_COMMANDS.filesystem.readDir: {
      const params = request.params as { path: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.readDir, {
        publicKey: extension.publicKey,
        name: extension.name,
        path: params.path,
      })
    }

    case TAURI_COMMANDS.filesystem.mkdir: {
      const params = request.params as { path: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.mkdir, {
        publicKey: extension.publicKey,
        name: extension.name,
        path: params.path,
      })
    }

    case TAURI_COMMANDS.filesystem.remove: {
      const params = request.params as { path: string; recursive?: boolean }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.remove, {
        publicKey: extension.publicKey,
        name: extension.name,
        path: params.path,
        recursive: params.recursive,
      })
    }

    case TAURI_COMMANDS.filesystem.exists: {
      const params = request.params as { path: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.exists, {
        publicKey: extension.publicKey,
        name: extension.name,
        path: params.path,
      })
    }

    case TAURI_COMMANDS.filesystem.stat: {
      const params = request.params as { path: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.stat, {
        publicKey: extension.publicKey,
        name: extension.name,
        path: params.path,
      })
    }

    case TAURI_COMMANDS.filesystem.selectFolder: {
      const params = request.params as { title?: string; defaultPath?: string }
      return invokeWithPermissionPrompt(
        TAURI_COMMANDS.filesystem.selectFolder,
        {
          publicKey: extension.publicKey,
          name: extension.name,
          title: params.title,
          defaultPath: params.defaultPath,
        },
      )
    }

    case TAURI_COMMANDS.filesystem.selectFile: {
      const params = request.params as {
        title?: string
        defaultPath?: string
        filters?: Array<[string, string[]]>
        multiple?: boolean
      }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.selectFile, {
        publicKey: extension.publicKey,
        name: extension.name,
        title: params.title,
        defaultPath: params.defaultPath,
        filters: params.filters,
        multiple: params.multiple,
      })
    }

    case TAURI_COMMANDS.filesystem.rename: {
      const params = request.params as { from: string; to: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.rename, {
        publicKey: extension.publicKey,
        name: extension.name,
        from: params.from,
        to: params.to,
      })
    }

    case TAURI_COMMANDS.filesystem.copy: {
      const params = request.params as { from: string; to: string }
      return invokeWithPermissionPrompt(TAURI_COMMANDS.filesystem.copy, {
        publicKey: extension.publicKey,
        name: extension.name,
        from: params.from,
        to: params.to,
      })
    }

    default:
      throw new Error(`Unknown filesystem method: ${request.method}`)
  }
}
