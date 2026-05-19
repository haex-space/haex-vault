import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type {
  PasswordItemFull,
  PasswordItemSummary,
  PasswordInput,
} from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

/**
 * Iframe handler for the core passwords vault.
 *
 * Adds the extension's `publicKey` and `name` to every invoke call —
 * the Rust side uses them to resolve the `extension_id` and apply the
 * tag-scope filter against the extension's `passwords` permissions.
 */
export async function handlePasswordsMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!extension) {
    throw new Error('Extension not found')
  }
  if (!request) {
    throw new Error('Request is required')
  }

  const { method, params = {} } = request
  const extInfo = {
    publicKey: extension.publicKey,
    name: extension.name,
  }

  switch (method) {
    case TAURI_COMMANDS.passwords.list: {
      return invokeWithPermissionPrompt<PasswordItemSummary[]>(
        TAURI_COMMANDS.passwords.list,
        { ...extInfo },
      )
    }

    case TAURI_COMMANDS.passwords.read: {
      const itemId = params.itemId as string
      if (!itemId) {
        throw new Error('itemId is required')
      }
      return invokeWithPermissionPrompt<PasswordItemFull>(
        TAURI_COMMANDS.passwords.read,
        { itemId, ...extInfo },
      )
    }

    case TAURI_COMMANDS.passwords.create: {
      const input = params.input as PasswordInput
      if (!input) {
        throw new Error('input is required')
      }
      return invokeWithPermissionPrompt<string>(
        TAURI_COMMANDS.passwords.create,
        { input, ...extInfo },
      )
    }

    case TAURI_COMMANDS.passwords.update: {
      const itemId = params.itemId as string
      const input = params.input as PasswordInput
      if (!itemId || !input) {
        throw new Error('itemId and input are required')
      }
      return invokeWithPermissionPrompt<null>(
        TAURI_COMMANDS.passwords.update,
        { itemId, input, ...extInfo },
      )
    }

    case TAURI_COMMANDS.passwords.delete: {
      const itemId = params.itemId as string
      if (!itemId) {
        throw new Error('itemId is required')
      }
      return invokeWithPermissionPrompt<null>(
        TAURI_COMMANDS.passwords.delete,
        { itemId, ...extInfo },
      )
    }

    default:
      throw new Error(`Unknown passwords method: ${method}`)
  }
}
