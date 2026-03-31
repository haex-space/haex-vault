import {
  type SharedSpace,
  SpaceRoles,
  type SpaceRole,
  type DecryptedSpace,
} from '@haex-space/vault-sdk'
import { eq, and } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaces } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { createRootUcanAsync, delegateUcanAsync, createServerRelayUcanAsync, persistUcanAsync, fetchWithUcanAuth, getUcanForSpaceAsync } from '@/utils/auth/ucanStore'

/** Extended space type including the DB type field (vault/shared/local) */
export interface SpaceWithType extends DecryptedSpace {
  type: 'vault' | 'shared' | 'local'
}

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const spaces = ref<SpaceWithType[]>([])
  const visibleSpaces = computed(() => spaces.value.filter(s => s.type !== 'vault'))

  // =========================================================================
  // DB Helpers
  // =========================================================================

  const getDb = () => currentVault.value?.drizzle

  /** Load all spaces from DB into memory */
  const loadSpacesFromDbAsync = async () => {
    const db = getDb()
    if (!db) return

    const rows = await db.select().from(haexSpaces)
    spaces.value = rows.map(rowToSpace)
  }

  /** Convert a DB row to SpaceWithType */
  const rowToSpace = (row: SelectHaexSpaces): SpaceWithType => ({
    id: row.id,
    name: row.name,
    type: (row.type as SpaceWithType['type']) ?? 'shared',
    role: row.role as SpaceRole,
    serverUrl: row.serverUrl ?? '',
    createdAt: row.createdAt ?? '',
  })

  /** Persist a space to DB and update in-memory list */
  const persistSpaceAsync = async (space: SpaceWithType) => {
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
        type: space.type,
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

    const space: SpaceWithType = {
      id,
      name: spaceName,
      type: 'local',
      role: SpaceRoles.ADMIN,
      serverUrl: '',
      createdAt: new Date().toISOString(),
    }

    await persistSpaceAsync(space)

    // Create MLS group for this space (enables epoch key encryption)
    await invoke('mls_create_group', { spaceId: id })
    await invoke('mls_export_epoch_key', { spaceId: id })

    // Create root UCAN for this space
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const identity = identityStore.identities[0]
    if (identity) {
      const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, id)
      const db = getDb()
      if (db) await persistUcanAsync(db, id, rootUcan)
    }

    log.info(`Created local space "${spaceName}" (${id})`)
    return { id }
  }

  const DEFAULT_SPACE_ID = 'default'

  const ensureVaultSpaceAsync = async (vaultId: string, vaultName: string) => {
    const db = getDb()
    if (!db) {
      console.error('[SPACES] ensureVaultSpaceAsync: no DB available')
      return
    }

    const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, vaultId)).limit(1)
    if (existing.length > 0) {
      log.info(`Vault space ${vaultId} already exists`)
      return
    }

    await db.insert(haexSpaces).values({
      id: vaultId,
      type: 'vault',
      name: vaultName,
      role: SpaceRoles.ADMIN,
      serverUrl: '',
    })
    log.info(`Created vault space "${vaultName}" (${vaultId})`)
  }

  const ensureDefaultSpaceAsync = async () => {
    const db = getDb()
    if (!db) return

    const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, DEFAULT_SPACE_ID)).limit(1)
    if (existing.length > 0) {
      // Ensure in-memory list is populated
      if (!spaces.value.find(s => s.id === DEFAULT_SPACE_ID)) {
        spaces.value.push(rowToSpace(existing[0]!))
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

    // Create MLS group for this space (enables epoch key encryption)
    await invoke('mls_create_group', { spaceId })
    await invoke('mls_export_epoch_key', { spaceId })

    // Create root UCAN for this space and persist to DB + cache
    const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, spaceId)
    const db = getDb()
    if (db) await persistUcanAsync(db, spaceId, rootUcan)

    // Upload initial KeyPackages so others can add us
    const { useMlsDelivery } = await import('@/composables/useMlsDelivery')
    const delivery = useMlsDelivery(serverUrl, spaceId, { privateKey: identity.privateKey, did: identity.did })
    await delivery.uploadKeyPackagesAsync()

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
    const spaceEntry = spaces.value.find(s => s.id === spaceId)
    if (spaceEntry?.type === 'local') throw new Error('Cannot change server for local spaces')
    if (spaceEntry?.type === 'vault') throw new Error('Cannot change server for vault space')

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

    const decrypted: SpaceWithType[] = rawSpaces.map((space) => ({
      id: space.id,
      name: space.encryptedName ?? `Space ${space.id.slice(0, 8)}`,
      type: 'shared' as const,
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

  /**
   * Create a pending invite for a DID to join a space.
   * The invitee accepts via PendingInvites UI, then the admin finalizes via finalizeInviteAsync.
   */
  const inviteMemberAsync = async (
    serverUrl: string,
    spaceId: string,
    inviteeDid: string,
    capability: string,
    identityId: string,
    includeHistory: boolean = false,
  ): Promise<{ inviteId: string }> => {
    const spaceEntry = spaces.value.find(s => s.id === spaceId)
    if (spaceEntry?.type === 'vault') throw new Error('Cannot invite members to vault space')

    const identity = await resolveIdentityAsync(identityId)

    // Create delegated UCAN for the invitee (signed by admin)
    const parentUcan = getUcanForSpaceAsync(spaceId)
    if (!parentUcan) throw new Error('No UCAN available for this space')

    const delegatedUcan = await delegateUcanAsync(
      identity.did,
      identity.privateKey,
      inviteeDid,
      spaceId,
      capability as any,
      parentUcan,
    )

    const response = await fetchWithSpaceUcanAuth(
      `${serverUrl}/spaces/${spaceId}/invites`,
      spaceId,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ inviteeDid, ucan: delegatedUcan, includeHistory }),
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to invite member: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    log.info(`Invited ${inviteeDid} to space ${spaceId} with ${capability}`)
    return { inviteId: data.invite.id }
  }

  /**
   * Create an invite token (for link or QR code sharing).
   * The token can be claimed by anyone with DID-Auth.
   */
  const createInviteTokenAsync = async (
    serverUrl: string,
    spaceId: string,
    options: {
      capability?: string
      maxUses?: number
      expiresInSeconds: number
      label?: string
    },
  ): Promise<{ tokenId: string; expiresAt: string }> => {
    const spaceEntry = spaces.value.find(s => s.id === spaceId)
    if (spaceEntry?.type === 'vault') throw new Error('Cannot create invite tokens for vault space')

    const response = await fetchWithSpaceUcanAuth(
      `${serverUrl}/spaces/${spaceId}/invite-tokens`,
      spaceId,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(options),
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to create invite token: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    log.info(`Created invite token for space ${spaceId} (maxUses: ${options.maxUses ?? 1})`)
    return { tokenId: data.token.id, expiresAt: data.token.expiresAt }
  }

  /**
   * Build an invite link from a token ID.
   */
  const buildInviteLink = (serverUrl: string, spaceId: string, tokenId: string): string => {
    const params = new URLSearchParams({ server: serverUrl, space: spaceId, token: tokenId })
    return `https://haex.space/invite?${params.toString()}`
  }

  /**
   * Claim an invite token (invitee side). Registers DID + uploads KeyPackages.
   * Handles both same-server and cross-server invites transparently.
   */
  const claimInviteTokenAsync = async (
    serverUrl: string,
    spaceId: string,
    tokenId: string,
    identityId: string,
  ): Promise<{ capability: string }> => {
    const identity = await resolveIdentityAsync(identityId)
    const relayServerUrl = detectCrossServerInvite(serverUrl)

    // Generate MLS KeyPackages
    const packages: number[][] = await invoke('mls_get_key_packages', { count: 10 })
    const keyPackagesBase64 = packages.map((p) => btoa(String.fromCharCode(...new Uint8Array(p))))

    // Claim the token on the home server (always direct, even for cross-server)
    const claimBody = JSON.stringify({
      keyPackages: keyPackagesBase64,
      label: identity.label,
    })
    const response = await fetchWithDidAuth(
      `${serverUrl}/spaces/${spaceId}/invite-tokens/${tokenId}/claim`,
      identity.privateKey,
      identity.did,
      'accept-invite',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: claimBody,
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to claim invite: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    const role = mapCapabilityToRole(data.capability)

    if (relayServerUrl) {
      // Cross-server: set up federation, then persist space pointing to relay
      await setupFederationForSpaceAsync(relayServerUrl, serverUrl, spaceId, identityId)

      await persistSpaceAsync({
        id: spaceId,
        name: '',
        type: 'shared',
        role,
        serverUrl: relayServerUrl,
        createdAt: new Date().toISOString(),
      })

      log.info(`Claimed cross-server invite for space ${spaceId} (capability: ${data.capability}, relay: ${relayServerUrl})`)
    } else {
      // Same-server: existing behavior
      await persistSpaceAsync({
        id: spaceId,
        name: '',
        type: 'shared',
        role,
        serverUrl,
        createdAt: new Date().toISOString(),
      })

      log.info(`Claimed invite token for space ${spaceId} (capability: ${data.capability})`)
    }

    return { capability: data.capability }
  }

  /**
   * Admin-side: finalize an accepted invite by adding the member to the MLS group.
   * For token invites (no UCAN yet), creates and attaches a delegated UCAN.
   * Fetches the invitee's KeyPackage, creates MLS add_member commit + welcome,
   * and sends both to the server.
   */
  const finalizeInviteAsync = async (
    serverUrl: string,
    spaceId: string,
    inviteeDid: string,
    identityId: string,
    inviteId?: string,
    capability?: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    const { useMlsDelivery } = await import('@/composables/useMlsDelivery')
    const delivery = useMlsDelivery(serverUrl, spaceId, { privateKey: identity.privateKey, did: identity.did })

    // 1. If no UCAN on the invite yet (token-based invite), create one now
    if (inviteId && capability) {
      const parentUcan = getUcanForSpaceAsync(spaceId)
      if (parentUcan) {
        const delegatedUcan = await delegateUcanAsync(
          identity.did,
          identity.privateKey,
          inviteeDid,
          spaceId,
          capability as any,
          parentUcan,
        )
        // Set UCAN on the invite (immutable, one-time only)
        await fetchWithSpaceUcanAuth(
          `${serverUrl}/spaces/${spaceId}/invites/${inviteId}/ucan`,
          spaceId,
          {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ ucan: delegatedUcan }),
          },
        )
      }
    }

    // 2. Fetch invitee's KeyPackage from server
    const { keyPackage } = await delivery.fetchKeyPackageAsync(inviteeDid)

    // 3. Add member to MLS group → produces commit + welcome
    const bundle = await invoke<{ commit: number[]; welcome: number[] | null; groupInfo: number[] }>('mls_add_member', {
      spaceId,
      keyPackage: Array.from(keyPackage),
    })

    // 4. Send commit to all group members
    await delivery.sendMessageAsync(new Uint8Array(bundle.commit), 'commit')

    // 5. Send welcome to the new member
    if (bundle.welcome) {
      await delivery.sendWelcomeAsync(inviteeDid, new Uint8Array(bundle.welcome))
    }

    log.info(`Finalized invite for ${inviteeDid} in space ${spaceId}`)
  }

  /**
   * Invitee-side: process MLS welcome messages to join the group.
   * Called after the admin finalizes the invite.
   */
  const processWelcomesAsync = async (serverUrl: string, spaceId: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    const { useMlsDelivery } = await import('@/composables/useMlsDelivery')
    const delivery = useMlsDelivery(serverUrl, spaceId, { privateKey: identity.privateKey, did: identity.did })

    const welcomes = await delivery.fetchWelcomesAsync()
    for (const welcome of welcomes) {
      await invoke('mls_process_message', { spaceId, message: Array.from(welcome) })
    }

    if (welcomes.length > 0) {
      log.info(`Processed ${welcomes.length} MLS welcome(s) for space ${spaceId}`)
    }
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
    const spaceEntry = spaces.value.find(s => s.id === spaceId)
    if (spaceEntry?.type === 'vault') throw new Error('Cannot delete vault space')

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

  // =========================================================================
  // Federation Helpers
  // =========================================================================

  const normalizeUrl = (url: string) => url.replace(/\/+$/, '').toLowerCase()

  /**
   * Check if an invite targets a different server than the user's own.
   * Returns the user's relay server URL if cross-server, null if same-server.
   */
  const detectCrossServerInvite = (homeServerUrl: string): string | null => {
    const backendsStore = useSyncBackendsStore()
    const userServerUrl = backendsStore.backends[0]?.serverUrl
    if (!userServerUrl) return null
    if (normalizeUrl(homeServerUrl) === normalizeUrl(userServerUrl)) return null
    return userServerUrl
  }

  /**
   * Fetch the relay server's federation DID.
   */
  const getRelayServerDidAsync = async (relayServerUrl: string): Promise<string> => {
    const response = await fetch(`${relayServerUrl}/federation/server-did`)
    if (!response.ok) throw new Error('Relay server does not support federation')
    const data = await response.json()
    return data.did
  }

  /**
   * Set up federation for a space: delegate server/relay UCAN to relay server,
   * then tell the relay server to establish federation with the home server.
   * Also creates a sync backend pointing to the relay server.
   */
  const setupFederationForSpaceAsync = async (
    relayServerUrl: string,
    homeServerUrl: string,
    spaceId: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)

    // 1. Get relay server's DID
    const relayServerDid = await getRelayServerDidAsync(relayServerUrl)

    // 2. Get our UCAN for this space (needed as proof)
    const parentUcan = getUcanForSpaceAsync(spaceId)
    if (!parentUcan) {
      log.warn('No UCAN for space yet — federation setup deferred')
      return
    }

    // 3. Create server/relay UCAN delegated to relay server
    const relayUcan = await createServerRelayUcanAsync(
      identity.did,
      identity.privateKey,
      relayServerDid,
      spaceId,
      parentUcan,
    )

    // 4. Tell relay server to establish federation with home server
    const response = await fetchWithDidAuth(
      `${relayServerUrl}/federation/setup`,
      identity.privateKey,
      identity.did,
      'federation-setup',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ spaceId, homeServerUrl, relayUcan }),
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to set up federation: ${error.error || response.statusText}`)
    }

    // 5. Create sync backend pointing to relay server
    const backendsStore = useSyncBackendsStore()
    const existingBackend = backendsStore.backends.find(b => b.spaceId === spaceId)
    if (!existingBackend) {
      await backendsStore.addBackendAsync({
        name: `Federation: ${spaceId.slice(0, 8)}`,
        serverUrl: relayServerUrl,
        spaceId,
        identityId: identity.publicKey,
        enabled: true,
        priority: 0,
      })
    }

    log.info(`Federation established: relay ${relayServerUrl} → home ${homeServerUrl} for space ${spaceId}`)
  }

  const mapCapabilityToRole = (capability: string): SpaceRole => {
    if (capability === 'space/admin') return SpaceRoles.ADMIN
    if (capability === 'space/read') return SpaceRoles.READER
    return SpaceRoles.MEMBER
  }

  const clearCache = () => {
    spaces.value = []
  }

  return {
    spaces,
    visibleSpaces,
    loadSpacesFromDbAsync,
    createLocalSpaceAsync,
    ensureVaultSpaceAsync,
    ensureDefaultSpaceAsync,
    createSpaceAsync,
    updateSpaceNameAsync,
    migrateSpaceServerAsync,
    listSpacesAsync,
    inviteMemberAsync,
    createInviteTokenAsync,
    buildInviteLink,
    claimInviteTokenAsync,
    finalizeInviteAsync,
    processWelcomesAsync,
    leaveSpaceAsync,
    deleteSpaceAsync,
    removeIdentityFromSpaceAsync,
    setupFederationForSpaceAsync,
    clearCache,
  }
})
