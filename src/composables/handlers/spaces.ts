// Handler for Space extension methods
// Row-assignment commands → Tauri invoke
// Space management commands → vault stores (crypto + HTTP)

import { invoke } from '@tauri-apps/api/core'
import { SPACE_COMMANDS } from '@haex-space/vault-sdk'
import type { DecryptedSpace, SyncBackendInfo } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'

export async function handleSpacesMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  switch (request.method) {
    case SPACE_COMMANDS.list:
      return handleListSpacesAsync()
    case SPACE_COMMANDS.create:
      return handleCreateSpaceAsync(request.params)
    case SPACE_COMMANDS.listBackends:
      return handleListBackendsAsync()
    default: {
      // Row assignment commands → forward to Tauri
      const params = {
        ...request.params,
        publicKey: extension.publicKey,
        name: extension.name,
      }
      return invoke(request.method, params)
    }
  }
}

async function handleListSpacesAsync(): Promise<DecryptedSpace[]> {
  const spacesStore = useSpacesStore()
  const backendsStore = useSyncBackendsStore()

  // Collect unique server URLs from personal backends
  const serverUrls = new Set<string>()
  for (const backend of backendsStore.backends) {
    if (backend.serverUrl && backend.type !== 'space') {
      serverUrls.add(backend.serverUrl)
    }
  }

  // Fetch and decrypt spaces from all servers
  const allSpaces: DecryptedSpace[] = []
  for (const serverUrl of serverUrls) {
    try {
      const decryptedSpaces = await spacesStore.listDecryptedSpacesAsync(serverUrl)
      allSpaces.push(...decryptedSpaces)
    } catch (error) {
      console.warn(`[SpacesHandler] Failed to list spaces from ${serverUrl}:`, error)
    }
  }

  return allSpaces
}

async function handleCreateSpaceAsync(
  params: Record<string, unknown>,
): Promise<DecryptedSpace> {
  const spacesStore = useSpacesStore()
  const name = params.name as string
  const serverUrl = params.server_url as string

  if (!name || !serverUrl) {
    throw new Error('Missing required params: name, server_url')
  }

  const createdSpace = await spacesStore.createSpaceAsync(serverUrl, name, name)

  return {
    id: createdSpace.id,
    name,
    role: createdSpace.role ?? 'admin',
    canInvite: createdSpace.canInvite ?? true,
    serverUrl,
    createdAt: createdSpace.createdAt,
  }
}

async function handleListBackendsAsync(): Promise<SyncBackendInfo[]> {
  const backendsStore = useSyncBackendsStore()

  // Only expose personal backends (not space backends)
  const personalBackends = backendsStore.backends.filter(
    (backend) => backend.type !== 'space' && backend.enabled,
  )

  // Backend with highest priority is the default
  const sorted = [...personalBackends].sort(
    (first, second) => (second.priority || 0) - (first.priority || 0),
  )
  const defaultBackendId = sorted[0]?.id

  return sorted.map((backend) => ({
    id: backend.id,
    name: backend.name,
    serverUrl: backend.serverUrl,
    isDefault: backend.id === defaultBackendId,
  }))
}
