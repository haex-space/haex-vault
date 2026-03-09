// Handler for Space assignment extension methods
// Routes extension_space_* commands to Tauri with extension identity

import { invoke } from '@tauri-apps/api/core'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'

export async function handleSpacesMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  const params = {
    ...request.params,
    publicKey: extension.publicKey,
    name: extension.name,
  }
  return invoke(request.method, params)
}
