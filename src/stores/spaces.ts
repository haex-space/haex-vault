import {
  type SharedSpace,
  SpaceRoles,
  type SpaceRole,
  type SpaceInvite,
  type DecryptedSpace,
} from '@haex-space/vault-sdk'
import { eq, and } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaces } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { createRootUcanAsync, cacheUcan, fetchWithUcanAuth, getUcanForSpaceAsync } from '@/utils/auth/ucanStore'

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const spaces = ref<DecryptedSpace[]>([])

  // =========================================================================
  // DB Helpers
  // =========================================================================

  const getDb = () => currentVault.value?.drizzle

  /** Load all spaces from DB into memory */
  const loadSpacesFromDbAsync = async () => {
    const db = getDb()
    if (!db) return

    const rows = await db.select().from(haexSpaces)
    spaces.value = rows.map(rowToDecryptedSpace)
  }

  /** Convert a DB row to DecryptedSpace */
  const rowToDecryptedSpace = (row: SelectHaexSpaces): DecryptedSpace => ({
    id: row.id,
    name: row.name,
    role: row.role as SpaceRole,
    serverUrl: row.serverUrl ?? '',
    createdAt: row.createdAt ?? '',
  })

  /** Persist a space to DB and update in-memory list */
  const persistSpaceAsync = async (space: DecryptedSpace) => {
    const db = getDb()
    if (!db) return

    const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, space.id)).limit(1)

    if (existing.length > 0) {
      await db.update(haexSpaces).set({
        name: space.name,
        serverUrl: space.serverUrl || null,
        role: space.role,
        modifiedAt: new Date().toISOString(),
      }).where(eq(haexSpaces.id, space.id))
    } else {
      await db.insert(haexSpaces).values({
        id: space.id,
        name: space.name,
        serverUrl: space.serverUrl || null,
        role: space.role,
      })
    }

    // Update in-memory
    const idx = spaces.value.findIndex(s => s.id === space.id)
    if (idx >= 0) {
      spaces.value[idx] = space
    } else {
      spaces.value.push(space)
    }
  }

  /** Remove a space from DB and in-memory list */
  const removeSpaceFromDbAsync = async (spaceId: string) => {
    const db = getDb()
    if (db) {
      await db.delete(haexSpaces).where(eq(haexSpaces.id, spaceId))
    }
    spaces.value = spaces.value.filter(s => s.id !== spaceId)
  }

  // =========================================================================
  // Auth Helpers
  // =========================================================================

  /** Fetch with UCAN authorization for space-scoped operations */
  const fetchWithSpaceUcanAuth = async (url: string, spaceId: string, options?: RequestInit) => {
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

  // =========================================================================
  // Identity Helper
  // =========================================================================

  const resolveIdentityAsync = async (identityId: string) => {
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityAsync(identityId)
    if (!identity) throw new Error(`Identity ${identityId} not found`)
    return identity
  }

  // =========================================================================
  // Space CRUD
  // =========================================================================

  const createLocalSpaceAsync = async (spaceName: string, spaceId?: string) => {
    const id = spaceId || crypto.randomUUID()

    const space: DecryptedSpace = {
      id,
      name: spaceName,
      role: SpaceRoles.ADMIN,
      serverUrl: '',
      createdAt: new Date().toISOString(),
    }

    await persistSpaceAsync(space)

    log.info(`Created local space "${spaceName}" (${id})`)
    return { id }
  }

  const DEFAULT_SPACE_ID = 'default'

  const ensureDefaultSpaceAsync = async () => {
    const db = getDb()
    if (!db) return

    const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, DEFAULT_SPACE_ID)).limit(1)
    if (existing.length > 0) {
      // Ensure in-memory list is populated
      if (!spaces.value.find(s => s.id === DEFAULT_SPACE_ID)) {
        spaces.value.push(rowToDecryptedSpace(existing[0]))
      }
      return
    }

    await createLocalSpaceAsync('Personal', DEFAULT_SPACE_ID)
    log.info('Default space created')
  }

  const createSpaceAsync = async (serverUrl: string, spaceName: string, selfLabel: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)

    const spaceId = crypto.randomUUID()

    const body = JSON.stringify({
      id: spaceId,
      name: spaceName,
      label: selfLabel,
    })
    const response = await fetchWithDidAuth(
      `${serverUrl}/spaces`,
      identity.privateKey,
      identity.did,
      'create-space',
      { method: 'POST', headers: { 'Content-Type': 'application/json' }, body },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to create space: ${error.error || JSON.stringify(error) || response.statusText}`)
    }

    // Create root UCAN for this space
    const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, spaceId)
    // TODO: Store in haex_ucan_tokens table (DB wiring comes later)
    cacheUcan(spaceId, rootUcan)

    log.info(`Created space ${spaceId}`)
    await listSpacesAsync(serverUrl, identityId)
    return { id: spaceId }
  }

  const updateSpaceNameAsync = async (spaceId: string, newName: string) => {
    const space = spaces.value.find(s => s.id === spaceId)
    if (!space) throw new Error('Space not found')

    if (space.serverUrl) {
      const response = await fetchWithSpaceUcanAuth(`${space.serverUrl}/spaces/${spaceId}`, spaceId, {
        method: 'PATCH',
        body: JSON.stringify({ name: newName }),
      })
      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        throw new Error(`Failed to update space name: ${error.error || response.statusText}`)
      }
    }

    await persistSpaceAsync({ ...space, name: newName })
    log.info(`Updated space "${spaceId}" name to "${newName}"`)
  }

  const migrateSpaceServerAsync = async (
    spaceId: string,
    oldServerUrl: string,
    newServerUrl: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)

    const space = spaces.value.find(s => s.id === spaceId)
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
      const body = JSON.stringify({
        id: spaceId,
        name: space.name,
        label: identity.label,
      })
      const response = await fetchWithDidAuth(
        `${newServerUrl}/spaces`,
        identity.privateKey,
        identity.did,
        'create-space',
        { method: 'POST', headers: { 'Content-Type': 'application/json' }, body },
      )

      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        throw new Error(`Failed to create space on new server: ${error.error || response.statusText}`)
      }
    }

    await persistSpaceAsync({ ...space, serverUrl: newServerUrl })
    log.info(`Migrated space "${spaceId}" from "${oldServerUrl || 'local'}" to "${newServerUrl || 'local'}"`)
  }

  const listSpacesAsync = async (serverUrl: string, identityId?: string) => {
    let response: Response
    if (identityId) {
      const identity = await resolveIdentityAsync(identityId)
      response = await fetchWithDidAuth(`${serverUrl}/spaces`, identity.privateKey, identity.did, 'list-spaces')
    } else {
      // Fallback: try first identity available
      const identityStore = useIdentityStore()
      const first = identityStore.identities[0]
      if (!first) throw new Error('No identity available for authentication')
      response = await fetchWithDidAuth(`${serverUrl}/spaces`, first.privateKey, first.did, 'list-spaces')
    }
    if (!response.ok) throw new Error('Failed to list spaces')
    const rawSpaces = await response.json() as SharedSpace[]

    const decrypted: DecryptedSpace[] = rawSpaces.map((space) => ({
      id: space.id,
      name: space.encryptedName ?? `Space ${space.id.slice(0, 8)}`,
      role: space.role,
      serverUrl,
      createdAt: space.createdAt,
    }))

    // Persist all remote spaces to local DB
    for (const space of decrypted) {
      await persistSpaceAsync(space)
    }

    return decrypted
  }

  const inviteMemberAsync = async (
    serverUrl: string,
    spaceId: string,
    inviteePublicKey: string,
    label: string,
    role: SpaceRole,
    _identityId: string,
  ): Promise<SpaceInvite> => {
    const memberResponse = await fetchWithSpaceUcanAuth(`${serverUrl}/spaces/${spaceId}/members`, spaceId, {
      method: 'POST',
      body: JSON.stringify({
        publicKey: inviteePublicKey,
        label,
        role,
      }),
    })

    if (!memberResponse.ok) {
      const error = await memberResponse.json().catch(() => ({}))
      throw new Error(`Failed to invite member: ${error.error || memberResponse.statusText}`)
    }

    const tokenResponse = await fetchWithSpaceUcanAuth(`${serverUrl}/spaces/${spaceId}/tokens`, spaceId, {
      method: 'POST',
      body: JSON.stringify({
        publicKey: inviteePublicKey,
        role,
        label: `Token for ${label}`,
      }),
    })

    if (!tokenResponse.ok) {
      throw new Error('Failed to create access token')
    }

    const { token: accessToken } = await tokenResponse.json()
    const spaceResponse = await fetchWithSpaceUcanAuth(`${serverUrl}/spaces/${spaceId}`, spaceId)
    const spaceData = await spaceResponse.json()

    // UCAN delegation for the invitee will be done after the invite is accepted (finalize flow)
    const invite: SpaceInvite = {
      spaceId,
      serverUrl,
      spaceName: spaceData.name ?? spaceData.encryptedName,
      accessToken,
      encryptedSpaceKey: '',
      keyNonce: '',
      ephemeralPublicKey: '',
      generation: 0,
      role,
    }

    log.info(`Invited ${label} (${inviteePublicKey.slice(0, 16)}...) to space ${spaceId} as ${role}`)
    return invite
  }

  const joinSpaceFromInviteAsync = async (invite: SpaceInvite, _identityId: string) => {
    // In Phase 4, joining is lightweight — no key decryption needed.
    // MLS key exchange will handle encryption in Phase 5.
    const space: DecryptedSpace = {
      id: invite.spaceId,
      name: invite.spaceName,
      role: invite.role,
      serverUrl: invite.serverUrl,
      createdAt: new Date().toISOString(),
    }

    await persistSpaceAsync(space)

    log.info(`Joined space ${invite.spaceId} via invite`)
    return { spaceId: invite.spaceId }
  }

  const leaveSpaceAsync = async (serverUrl: string, spaceId: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)

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

  const deleteSpaceAsync = async (serverUrl: string, spaceId: string) => {
    const response = await fetchWithSpaceUcanAuth(`${serverUrl}/spaces/${spaceId}`, spaceId, {
      method: 'DELETE',
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to delete space: ${error.error || response.statusText}`)
    }

    await removeSpaceFromDbAsync(spaceId)
    log.info(`Deleted space ${spaceId}`)
  }

  const removeIdentityFromSpaceAsync = async (spaceId: string, identityPublicKey: string) => {
    const db = getDb()
    if (!db) throw new Error('No vault open')

    const { haexSpaceDevices } = await import('~/database/schemas')

    await db.delete(haexSpaceDevices)
      .where(and(
        eq(haexSpaceDevices.spaceId, spaceId),
        eq(haexSpaceDevices.identityId, identityPublicKey),
      ))

    const space = spaces.value.find(s => s.id === spaceId)
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

  const clearCache = () => {
    spaces.value = []
  }

  return {
    spaces,
    loadSpacesFromDbAsync,
    createLocalSpaceAsync,
    ensureDefaultSpaceAsync,
    createSpaceAsync,
    updateSpaceNameAsync,
    migrateSpaceServerAsync,
    listSpacesAsync,
    inviteMemberAsync,
    joinSpaceFromInviteAsync,
    leaveSpaceAsync,
    deleteSpaceAsync,
    removeIdentityFromSpaceAsync,
    clearCache,
  }
})
