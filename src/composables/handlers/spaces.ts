// Handler for Space extension methods
// All commands are forwarded to Tauri backend with permission prompt handling

import { invoke } from '@tauri-apps/api/core'
import { SPACE_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import {
  isPermissionPromptRequired,
  extractPromptData,
} from '~/composables/usePermissionPrompt'

const { promptForPermission } = usePermissionPrompt()

/**
 * Invoke a Tauri command with permission prompt handling.
 * If the command fails with PermissionPromptRequired, waits for user decision and retries.
 */
async function invokeWithPermission<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error: unknown) {
    if (isPermissionPromptRequired(error)) {
      const data = extractPromptData(error)!
      const decision = await promptForPermission(data)

      if (decision === 'granted') {
        return await invoke<T>(command, args)
      }
    }
    throw error
  }
}

export async function handleSpacesMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  // The create command needs param remapping because the SDK sends `name`
  // for the space name, but Tauri also needs `name` for extension identity.
  if (request.method === SPACE_COMMANDS.create) {
    return invokeWithPermission(request.method, {
      spaceName: request.params.name as string,
      serverUrl: request.params.serverUrl as string,
      publicKey: extension.publicKey,
      name: extension.name,
    })
  }

  const params = {
    ...request.params,
    publicKey: extension.publicKey,
    name: extension.name,
  }
  return invokeWithPermission(request.method, params)
}
