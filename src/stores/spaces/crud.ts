import { and, eq, inArray, sql } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import {
  haexDeletedRows,
  haexSpaceDevices,
  haexSpaceMembers,
  haexSpaces,
  haexUcanTokens,
} from '~/database/schemas'
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
    originUrl: '',
    createdAt: new Date().toISOString(),
    capabilities: [],
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
  originUrl: string,
  spaceName: string,
  selfLabel: string,
  identity: ResolvedIdentity,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
  listSpaces: () => Promise<void>,
): Promise<{ id: string }> {
  const spaceId = crypto.randomUUID()

  const body = JSON.stringify({ id: spaceId, name: spaceName, label: selfLabel })
  const response = await fetchWithDidAuth(
    `${originUrl}/spaces`,
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
    originUrl: originUrl,
    status: SpaceStatus.ACTIVE,
  })

  await ensureMlsGroupAsync(spaceId)
  await invoke('mls_export_epoch_key', { spaceId })

  const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, spaceId)
  if (db) await persistUcanAsync(db, spaceId, rootUcan)

  const { useMlsDelivery } = await import('@/composables/useMlsDelivery')
  const delivery = useMlsDelivery(originUrl, spaceId, { privateKey: identity.privateKey, did: identity.did })
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
    originUrl: originUrl,
    createdAt: new Date().toISOString(),
    capabilities: [],
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

  if (space.originUrl) {
    const response = await fetchWithSpaceUcanAuth(`${space.originUrl}/spaces/${spaceId}`, spaceId, {
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

  await persistSpaceAsync({ ...space, originUrl: newServerUrl })
  log.info(`Migrated space "${spaceId}" from "${oldServerUrl || 'local'}" to "${newServerUrl || 'local'}"`)
}

export async function listSpaces(
  identity: ResolvedIdentity,
  originUrl: string,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
): Promise<SpaceWithType[]> {
  const response = await fetchWithDidAuth(`${originUrl}/spaces`, identity.privateKey, identity.did, 'list-spaces')
  await throwIfNotOk(response, 'list spaces')
  const rawSpaces = await response.json() as Array<{ id: string; encryptedName?: string; createdAt: string }>

  const decrypted: SpaceWithType[] = rawSpaces.map((space) => ({
    id: space.id,
    name: space.encryptedName ?? `Space ${space.id.slice(0, 8)}`,
    type: SpaceType.ONLINE,
    status: SpaceStatus.ACTIVE,
    ownerIdentityId: identity.id,
    originUrl: originUrl,
    createdAt: space.createdAt,
    capabilities: [],
  }))

  for (const space of decrypted) {
    await persistSpaceAsync(space)
  }

  return decrypted
}

export async function leaveSpace(
  identity: ResolvedIdentity,
  originUrl: string,
  spaceId: string,
  removeSpaceFromDbAsync: (spaceId: string) => Promise<void>,
) {
  const response = await fetchWithDidAuth(
    `${originUrl}/spaces/${spaceId}/members/${encodeURIComponent(identity.publicKey)}`,
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

/**
 * How long after marking a space LEAVING we still keep the haex_spaces row
 * around so the per-space sync loop can flush its delete-log entries to
 * the leader. After this window we give up regardless of delivery status.
 */
export const LEAVE_GIVE_UP_AFTER_MS = 30 * 24 * 60 * 60 * 1000

/**
 * Remove `haex_spaces` rows for spaces in LEAVING state where we are
 * confident the delete-log entries (member row + UCAN tokens) have made
 * it to the leader.
 *
 * "Confident" is a *heuristic* — distributed systems give us no native ACK
 * for CRDT propagation. The decision below is the policy knob.
 *
 * @returns the number of spaces actually removed.
 */
export async function cleanupCompletedLeavesAsync(
  db: DB,
  removeSpaceFromDbAsync: (spaceId: string) => Promise<void>,
): Promise<number> {
  // 1. Find all LEAVING candidates. We carry their `modifiedAt` (bumped to
  //    "now" at the moment of leave) so the heuristic below can reason about
  //    age.
  const candidates = await db
    .select({
      id: haexSpaces.id,
      modifiedAt: haexSpaces.modifiedAt,
    })
    .from(haexSpaces)
    .where(eq(haexSpaces.status, SpaceStatus.LEAVING))

  if (candidates.length === 0) return 0

  // 2. For each candidate, decide if it's safe to fully remove.
  //
  // Time-based give-up: 30 days after the leave transition we drop the row
  // unconditionally. This is *not* a delivery confirmation — it is a
  // resource-bound. Either the push reached the leader during that window
  // (typical case) or the leader has been unreachable for a month, in
  // which case the membership relationship is effectively dead anyway.
  //
  // The window is intentionally generous because the cost of waiting is a
  // single hidden DB row per departed space; the cost of cleaning too
  // early is the leader never seeing our leave at all.
  const isLeaveSafeToFinalize = (candidate: {
    id: string
    modifiedAt: string | null
  }): boolean => {
    if (!candidate.modifiedAt) return false
    const ageMs = Date.now() - new Date(candidate.modifiedAt).getTime()
    if (Number.isNaN(ageMs)) return false
    return ageMs > LEAVE_GIVE_UP_AFTER_MS
  }

  let removed = 0
  for (const c of candidates) {
    if (!isLeaveSafeToFinalize(c)) continue
    try {
      await removeSpaceFromDbAsync(c.id)
      log.info(`Finalized LEAVING space ${c.id} (delete-log assumed propagated)`)
      removed += 1
    } catch (error) {
      log.warn(`Failed to finalize LEAVING space ${c.id}: ${error}`)
    }
  }
  return removed
}

export async function deleteSpace(
  spaces: SpaceWithType[],
  originUrl: string,
  spaceId: string,
  removeSpaceFromDbAsync: (spaceId: string) => Promise<void>,
) {
  const spaceEntry = spaces.find(s => s.id === spaceId)
  if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot delete vault space')

  const response = await fetchWithSpaceUcanAuth(`${originUrl}/spaces/${spaceId}`, spaceId, {
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
  if (space?.originUrl) {
    try {
      const response = await fetchWithSpaceUcanAuth(
        `${space.originUrl}/spaces/${spaceId}/members/${encodeURIComponent(identityPublicKey)}`,
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
