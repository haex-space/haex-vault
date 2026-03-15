// Handler for Space extension methods
//
// list, create, and listBackends are handled by frontend stores (useSpacesStore,
// useSyncBackendsStore) which use the current Supabase JWT auth.
// assign, unassign, getAssignments are forwarded to Tauri (local DB only, no auth needed).

import { invoke } from '@tauri-apps/api/core'
import { SPACE_COMMANDS } from '@haex-space/vault-sdk'
import type { DecryptedSpace, SyncBackendInfo } from '@haex-space/vault-sdk'
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
  // ── list: fetch spaces from all enabled backends via frontend store ──
  if (request.method === SPACE_COMMANDS.list) {
    const spacesStore = useSpacesStore()
    const backendsStore = useSyncBackendsStore()

    const allSpaces: DecryptedSpace[] = []
    for (const backend of backendsStore.enabledBackends) {
      if (!backend.serverUrl) continue
      try {
        const spaces = await spacesStore.listSpacesAsync(backend.serverUrl)
        allSpaces.push(...spaces)
      } catch (e) {
        console.warn(`[Spaces] Failed to list spaces from ${backend.serverUrl}:`, e)
      }
    }
    return allSpaces
  }

  // ── create: delegate to frontend store with identity from backend ──
  if (request.method === SPACE_COMMANDS.create) {
    const spacesStore = useSpacesStore()
    const backendsStore = useSyncBackendsStore()

    const serverUrl = request.params.serverUrl as string
    const spaceName = request.params.name as string

    const backend = await backendsStore.findBackendByServerUrlAsync(serverUrl)
    if (!backend?.identityId) {
      throw new Error(`No identity linked to backend for ${serverUrl}`)
    }

    const result = await spacesStore.createSpaceAsync(serverUrl, spaceName, spaceName, backend.identityId)

    // Return in DecryptedSpace format expected by extensions
    const created = spacesStore.spaces.find(s => s.id === result.id)
    return created ?? {
      id: result.id,
      name: spaceName,
      role: 'admin',
      serverUrl,
      createdAt: new Date().toISOString(),
    } satisfies DecryptedSpace
  }

  // ── listBackends: read enabled backends directly from frontend store ──
  if (request.method === SPACE_COMMANDS.listBackends) {
    const backendsStore = useSyncBackendsStore()

    const sorted = [...backendsStore.enabledBackends]
      .filter(b => b.serverUrl)
      .sort((a, b) => (b.priority || 0) - (a.priority || 0))

    return sorted.map((b, i): SyncBackendInfo => ({
      id: b.id,
      name: b.name || '',
      serverUrl: b.serverUrl || '',
      isDefault: i === 0,
    }))
  }

  // ── assign, unassign, getAssignments → Tauri (local DB, no auth) ──
  const params = {
    ...request.params,
    publicKey: extension.publicKey,
    name: extension.name,
  }
  return invokeWithPermission(request.method, params)
}
