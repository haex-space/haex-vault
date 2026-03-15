// Handler for Space extension methods
// All commands are forwarded to Tauri backend with permission prompt handling

import { SPACE_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

export async function handleSpacesMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  // The create command needs param remapping because the SDK sends `name`
  // for the space name, but Tauri also needs `name` for extension identity.
  if (request.method === SPACE_COMMANDS.create) {
    return invokeWithPermissionPrompt(request.method, {
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
  return invokeWithPermissionPrompt(request.method, params)
}
