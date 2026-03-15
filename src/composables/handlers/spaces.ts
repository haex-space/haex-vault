// Handler for Space extension methods
// All commands are forwarded to Tauri backend with permission prompt handling

import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

export async function handleSpacesMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  const params = {
    ...request.params,
    publicKey: extension.publicKey,
    name: extension.name,
  }
  return invokeWithPermissionPrompt(request.method, params)
}
