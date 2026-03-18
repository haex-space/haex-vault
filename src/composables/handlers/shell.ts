import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

export async function handleShellMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!extension || !request) {
    throw new Error('Extension not found')
  }

  const { method, params } = request

  switch (method) {
    case TAURI_COMMANDS.shell.create: {
      const options = params.options as Record<string, unknown> | undefined
      return invokeWithPermissionPrompt(TAURI_COMMANDS.shell.create, {
        publicKey: extension.publicKey,
        name: extension.name,
        options: options ?? {},
      })
    }

    case TAURI_COMMANDS.shell.write: {
      return invokeWithPermissionPrompt(TAURI_COMMANDS.shell.write, {
        publicKey: extension.publicKey,
        name: extension.name,
        sessionId: params.sessionId as string,
        data: params.data as string,
      })
    }

    case TAURI_COMMANDS.shell.resize: {
      return invokeWithPermissionPrompt(TAURI_COMMANDS.shell.resize, {
        publicKey: extension.publicKey,
        name: extension.name,
        sessionId: params.sessionId as string,
        cols: params.cols as number,
        rows: params.rows as number,
      })
    }

    case TAURI_COMMANDS.shell.close: {
      return invokeWithPermissionPrompt(TAURI_COMMANDS.shell.close, {
        publicKey: extension.publicKey,
        name: extension.name,
        sessionId: params.sessionId as string,
      })
    }

    default:
      throw new Error(`Unknown shell method: ${method}`)
  }
}
