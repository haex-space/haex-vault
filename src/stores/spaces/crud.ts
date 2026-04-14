import { eq } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaceDevices, haexSpaces } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { createRootUcanAsync, persistUcanAsync, fetchWithUcanAuth, getUcanForSpaceAsync } from '@/utils/auth/ucanStore'
import { throwIfNotOk } from '@/utils/fetch'
import { SpaceType, SpaceStatus } from '~/database/constants'
import { createLogger } from '@/stores/logging'
import { addSelfAsSpaceMember } from './members'
import type { SpaceWithType, ResolvedIdentity } from './index'

type DB = SqliteRemoteDatabase<typeof schema>

const log = createLogger('SPACES:CRUD')

/** Fetch with UCAN authorization for space-scoped operations */
function fetchWithSpaceUcanAuth(url: string, spaceId: string, options?: RequestInit) {
  const ucan = getUcanForSpaceAsync(spaceId)
  if (!ucan) throw new Error(`No UCAN token available for space ${spaceId}`)
  return fetchWithUcanAuth(url, spaceId, ucan, {
    ...options,
    headers: {
      ...options?.headers,
      'Content-Type': 'application/json',
    },
  })
}

async function ensureMlsGroupAsync(spaceId: string) {
  const hasGroup = await invoke<boolean>('mls_has_group', { spaceId })
  if (!hasGroup) {
    await invoke('mls_create_group', { spaceId })
  }
}

export async function createLocalSpace(
  db: DB,
  spaceName: string,
  ownerIdentityId: string,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
  spaceId?: string,
): Promise<{ id: string }> {
  const id = spaceId || crypto.randomUUID()

  const space: SpaceWithType = {
    id,
    name: spaceName,
    type: SpaceType.LOCAL,
    status: SpaceStatus.ACTIVE,
    ownerIdentityId,
    serverUrl: '',
    createdAt: new Date().toISOString(),
  }

  const identityStore = useIdentityStore()
  await identityStore.loadIdentitiesAsync()
  const identity = identityStore.ownIdentities.find(i => i.id === ownerIdentityId)
  if (!identity) throw new Error('Selected owner identity not available')

  // Persist the space before MLS stores its FK-backed epoch sync key.
  await db.insert(haexSpaces).values({
    id,
    type: SpaceType.LOCAL,
    name: spaceName,
    ownerIdentityId,
    originUrl: null,
    status: SpaceStatus.ACTIVE,
  })

  await ensureMlsGroupAsync(id)
  await invoke('mls_export_epoch_key', { spaceId: id })

  // Create admin UCAN (must exist before UI renders SpaceListItem)
  if (identity.privateKey) {
    const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, id)
    await persistUcanAsync(db, id, rootUcan)
  }

  // Push to reactive list — SpaceListItem.onMounted will find the UCAN
  await persistSpaceAsync(space)

  // Add creator as space member (non-fatal — space must work even if member insert fails)
  if (identity) {
    await addSelfAsSpaceMember(db, id, identity, 'admin')
  }

  await invoke('local_delivery_start', { spaceId: id })

  // Register this device in the new space so PushInvites to contacts carry
  // a usable spaceEndpoints list. autoRegisterInSpacesAsync only runs when
  // peer_storage starts; a runtime-created space would otherwise stay
  // unregistered until the next app restart.
  const peerStorageStore = usePeerStorageStore()
  if (peerStorageStore.nodeId) {
    try {
      const deviceStore = useDeviceStore()
      const deviceName = deviceStore.deviceName || deviceStore.hostname || 'Unknown'
      await peerStorageStore.registerDeviceInSpaceAsync(id, deviceName)
    } catch (error) {
      log.warn(`Failed to register device in new space ${id}: ${error}`)
    }
  }

  log.info(`Created local space "${spaceName}" (${id})`)
  return { id }
}

export async function createOnlineSpace(
  db: DB,
  serverUrl: string,
  spaceName: string,
  selfLabel: string,
  identity: ResolvedIdentity,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
  listSpaces: () => Promise<void>,
): Promise<{ id: string }> {
  const spaceId = crypto.randomUUID()

  const body = JSON.stringify({ id: spaceId, name: spaceName, label: selfLabel })
  const response = await fetchWithDidAuth(
    `${serverUrl}/spaces`,
    identity.privateKey,
    identity.did,
    'create-space',
    { method: 'POST', headers: { 'Content-Type': 'application/json' }, body },
  )

  await throwIfNotOk(response, 'create space')

  await db.insert(haexSpaces).values({
    id: spaceId,
    type: SpaceType.ONLINE,
    name: spaceName,
    ownerIdentityId: identity.id,
    originUrl: serverUrl,
    status: SpaceStatus.ACTIVE,
  })

  await ensureMlsGroupAsync(spaceId)
  await invoke('mls_export_epoch_key', { spaceId })

  const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, spaceId)
  if (db) await persistUcanAsync(db, spaceId, rootUcan)

  const { useMlsDelivery } = await import('@/composables/useMlsDelivery')
  const delivery = useMlsDelivery(serverUrl, spaceId, { privateKey: identity.privateKey, did: identity.did })
  await delivery.uploadKeyPackagesAsync()

  // Add creator as space member (non-fatal)
  const identityStore = useIdentityStore()
  const fullIdentity = identityStore.ownIdentities.find(i => i.did === identity.did)
  if (fullIdentity) {
    await addSelfAsSpaceMember(db, spaceId, fullIdentity, 'admin')
  }

  await persistSpaceAsync({
    id: spaceId,
    name: spaceName,
    type: SpaceType.ONLINE,
    status: SpaceStatus.ACTIVE,
    ownerIdentityId: identity.id,
    serverUrl,
    createdAt: new Date().toISOString(),
  })

  log.info(`Created space ${spaceId}`)
  await listSpaces()
  return { id: spaceId }
}

export async function updateSpaceName(
  spaces: SpaceWithType[],
  spaceId: string,
  newName: string,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
) {
  const space = spaces.find(s => s.id === spaceId)
  if (!space) throw new Error('Space not found')

  if (space.serverUrl) {
    const response = await fetchWithSpaceUcanAuth(`${space.serverUrl}/spaces/${spaceId}`, spaceId, {
      method: 'PATCH',
      body: JSON.stringify({ name: newName }),
    })
    await throwIfNotOk(response, 'update space name')
  }

  await persistSpaceAsync({ ...space, name: newName })
  log.info(`Updated space "${spaceId}" name to "${newName}"`)
}

export async function migrateSpaceServer(
  spaces: SpaceWithType[],
  spaceId: string,
  oldServerUrl: string,
  newServerUrl: string,
  identity: ResolvedIdentity,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
) {
  const spaceEntry = spaces.find(s => s.id === spaceId)
  if (spaceEntry?.type === SpaceType.LOCAL) throw new Error('Cannot change server for local spaces')
  if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot change server for vault space')

  const space = spaces.find(s => s.id === spaceId)
  if (!space) throw new Error('Space not found')

  if (oldServerUrl) {
    try {
      const response = await fetchWithSpaceUcanAuth(`${oldServerUrl}/spaces/${spaceId}`, spaceId, {
        method: 'DELETE',
      })
      if (!response.ok && response.status !== 404) {
        log.warn(`Failed to delete space from old server (${response.status}), proceeding anyway`)
      }
    } catch (e) {
      log.warn(`Old server unreachable, space may remain there: ${e}`)
    }
  }

  if (newServerUrl) {
    const body = JSON.stringify({ id: spaceId, name: space.name, label: identity.name })
    const response = await fetchWithDidAuth(
      `${newServerUrl}/spaces`,
      identity.privateKey,
      identity.did,
      'create-space',
      { method: 'POST', headers: { 'Content-Type': 'application/json' }, body },
    )

    await throwIfNotOk(response, 'create space on new server')
  }

  await persistSpaceAsync({ ...space, serverUrl: newServerUrl })
  log.info(`Migrated space "${spaceId}" from "${oldServerUrl || 'local'}" to "${newServerUrl || 'local'}"`)
}

export async function listSpaces(
  identity: ResolvedIdentity,
  serverUrl: string,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
): Promise<SpaceWithType[]> {
  const response = await fetchWithDidAuth(`${serverUrl}/spaces`, identity.privateKey, identity.did, 'list-spaces')
  await throwIfNotOk(response, 'list spaces')
  const rawSpaces = await response.json() as Array<{ id: string; encryptedName?: string; createdAt: string }>

  const decrypted: SpaceWithType[] = rawSpaces.map((space) => ({
    id: space.id,
    name: space.encryptedName ?? `Space ${space.id.slice(0, 8)}`,
    type: SpaceType.ONLINE,
    status: SpaceStatus.ACTIVE,
    ownerIdentityId: identity.id,
    serverUrl,
    createdAt: space.createdAt,
  }))

  for (const space of decrypted) {
    await persistSpaceAsync(space)
  }

  return decrypted
}

export async function leaveSpace(
  identity: ResolvedIdentity,
  serverUrl: string,
  spaceId: string,
  removeSpaceFromDbAsync: (spaceId: string) => Promise<void>,
) {
  const response = await fetchWithDidAuth(
    `${serverUrl}/spaces/${spaceId}/members/${encodeURIComponent(identity.publicKey)}`,
    identity.privateKey,
    identity.did,
    'leave-space',
    { method: 'DELETE' },
  )

  if (!response.ok && response.status !== 404) {
    throw new Error('Failed to leave space')
  }

  await removeSpaceFromDbAsync(spaceId)
  log.info(`Left space ${spaceId}`)
}

export async function deleteSpace(
  spaces: SpaceWithType[],
  serverUrl: string,
  spaceId: string,
  removeSpaceFromDbAsync: (spaceId: string) => Promise<void>,
) {
  const spaceEntry = spaces.find(s => s.id === spaceId)
  if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot delete vault space')

  const response = await fetchWithSpaceUcanAuth(`${serverUrl}/spaces/${spaceId}`, spaceId, {
    method: 'DELETE',
  })

  await throwIfNotOk(response, 'delete space')

  await removeSpaceFromDbAsync(spaceId)
  log.info(`Deleted space ${spaceId}`)
}

export async function removeIdentityFromSpace(
  db: DB,
  spaces: SpaceWithType[],
  spaceId: string,
  identityPublicKey: string,
) {
  await db.delete(haexSpaceDevices)
    .where(eq(haexSpaceDevices.spaceId, spaceId))

  const space = spaces.find(s => s.id === spaceId)
  if (space?.serverUrl) {
    try {
      const response = await fetchWithSpaceUcanAuth(
        `${space.serverUrl}/spaces/${spaceId}/members/${encodeURIComponent(identityPublicKey)}`,
        spaceId,
        { method: 'DELETE' },
      )
      if (!response.ok && response.status !== 404) {
        log.warn(`Failed to remove member from server (${response.status})`)
      }
    } catch (e) {
      log.warn(`Server unreachable, member removed locally only: ${e}`)
    }
  }

  try {
    await invoke('peer_storage_reload_shares')
  } catch {
    // P2P endpoint may not be running
  }

  log.info(`Removed identity ${identityPublicKey.slice(0, 20)}... from space ${spaceId}`)
}
