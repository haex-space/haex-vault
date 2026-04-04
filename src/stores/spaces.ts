import type { DecryptedSpace } from '@haex-space/vault-sdk'
import { eq, and } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaces, haexUcanTokens, haexInviteTokens } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { createRootUcanAsync, delegateUcanAsync, createServerRelayUcanAsync, persistUcanAsync, fetchWithUcanAuth, getUcanForSpaceAsync } from '@/utils/auth/ucanStore'
import { SpaceType, SpaceStatus } from '~/database/constants'
import type { SpaceType as SpaceTypeValue, SpaceStatus as SpaceStatusValue } from '~/database/constants'
import spacesDe from './spaces.de.json'
import spacesEn from './spaces.en.json'

/** Extended space type including the DB type field (vault/online/local) */
export interface SpaceWithType extends DecryptedSpace {
  type: SpaceTypeValue
  status: SpaceStatusValue
}

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const { $i18n } = useNuxtApp()
  $i18n.mergeLocaleMessage('de', { spaces: spacesDe })
  $i18n.mergeLocaleMessage('en', { spaces: spacesEn })

  const { currentVault } = storeToRefs(useVaultStore())

  const spaces = ref<SpaceWithType[]>([])
  const visibleSpaces = computed(() => spaces.value.filter(s => s.type !== SpaceType.VAULT))
  const activeSpaces = computed(() => visibleSpaces.value.filter(s => s.status === SpaceStatus.ACTIVE))
  const pendingSpaces = computed(() => visibleSpaces.value.filter(s => s.status === SpaceStatus.PENDING))

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

  /** Start leader mode for all active local spaces (call once on app startup) */
  const startLocalSpaceLeadersAsync = async () => {
    for (const space of spaces.value) {
      if (space.type === SpaceType.LOCAL && space.status === SpaceStatus.ACTIVE) {
        try {
          await invoke('local_delivery_start', { spaceId: space.id })
          log.info(`Started leader mode for local space ${space.id}`)
        } catch {
          // Already running — ignore
        }
      }
    }
  }

  /** Convert a DB row to SpaceWithType */
  const rowToSpace = (row: SelectHaexSpaces): SpaceWithType => ({
    id: row.id,
    name: row.name,
    type: (row.type as SpaceTypeValue) ?? SpaceType.ONLINE,
    status: (row.status as SpaceStatusValue) ?? SpaceStatus.ACTIVE,
    serverUrl: row.originUrl ?? '',
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
        originUrl: space.serverUrl || null,
        status: space.status,
        modifiedAt: new Date().toISOString(),
      }).where(eq(haexSpaces.id, space.id))
    } else {
      await db.insert(haexSpaces).values({
        id: space.id,
        type: space.type,
        name: space.name,
        originUrl: space.serverUrl || null,
        status: space.status,
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

  const resolveIdentityAsync = async (identityId: string): Promise<{ id: string; publicKey: string; privateKey: string; did: string; label: string }> => {
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityByIdAsync(identityId)
    if (!identity?.privateKey) throw new Error(`Identity ${identityId} not found or has no private key`)
    return { id: identity.id, publicKey: identity.publicKey, privateKey: identity.privateKey, did: identity.did, label: identity.label }
  }

  // =========================================================================
  // Space CRUD
  // =========================================================================

  const createLocalSpaceAsync = async (spaceName: string, spaceId?: string) => {
    const id = spaceId || crypto.randomUUID()

    const space: SpaceWithType = {
      id,
      name: spaceName,
      type: SpaceType.LOCAL,
      status: SpaceStatus.ACTIVE,
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
    const identity = identityStore.ownIdentities[0]
    if (identity?.privateKey) {
      const rootUcan = await createRootUcanAsync(identity.did, identity.privateKey, id)
      const db = getDb()
      if (db) await persistUcanAsync(db, id, rootUcan)
    }

    // Start leader mode so this device can handle invites and delivery
    await invoke('local_delivery_start', { spaceId: id })

    log.info(`Created local space "${spaceName}" (${id})`)
    return { id }
  }

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
      type: SpaceType.VAULT,
      name: vaultName,
      originUrl: '',
    })
    log.info(`Created vault space "${vaultName}" (${vaultId})`)
  }

  const ensureDefaultSpaceAsync = async () => {
    const db = getDb()
    if (!db) return

    // Check if any local space already exists (regardless of name/ID)
    const localSpaces = await db
      .select()
      .from(haexSpaces)
      .where(eq(haexSpaces.type, SpaceType.LOCAL))
      .limit(1)

    if (localSpaces.length > 0) {
      if (!spaces.value.find(s => s.id === localSpaces[0]!.id)) {
        spaces.value.push(rowToSpace(localSpaces[0]!))
      }
      return
    }

    // No local space yet — create one with a random UUID and localized name
    const name = $i18n.t('spaces.defaultSpaceName')
    await createLocalSpaceAsync(name)
    log.info(`Default space "${name}" created`)
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
    if (spaceEntry?.type === SpaceType.LOCAL) throw new Error('Cannot change server for local spaces')
    if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot change server for vault space')

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

  const listSpacesAsync = async (serverUrl: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    const response = await fetchWithDidAuth(`${serverUrl}/spaces`, identity.privateKey, identity.did, 'list-spaces')
    if (!response.ok) throw new Error('Failed to list spaces')
    const rawSpaces = await response.json() as Array<{ id: string; encryptedName?: string; createdAt: string }>

    const decrypted: SpaceWithType[] = rawSpaces.map((space) => ({
      id: space.id,
      name: space.encryptedName ?? `Space ${space.id.slice(0, 8)}`,
      type: SpaceType.ONLINE,
      status: SpaceStatus.ACTIVE,
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
    if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot invite members to vault space')

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
    if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot create invite tokens for vault space')

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

    // Claim the token on the origin server (always direct, even for cross-server)
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

    if (relayServerUrl) {
      // Cross-server: set up federation, then persist space pointing to relay
      await setupFederationForSpaceAsync(relayServerUrl, serverUrl, spaceId, identityId)

      await persistSpaceAsync({
        id: spaceId,
        name: '',
        type: SpaceType.ONLINE,
        status: SpaceStatus.ACTIVE,
        serverUrl: relayServerUrl,
        createdAt: new Date().toISOString(),
      })

      log.info(`Claimed cross-server invite for space ${spaceId} (capability: ${data.capability}, relay: ${relayServerUrl})`)
    } else {
      // Same-server: existing behavior
      await persistSpaceAsync({
        id: spaceId,
        name: '',
        type: SpaceType.ONLINE,
        status: SpaceStatus.ACTIVE,
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
    if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot delete vault space')

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
  const detectCrossServerInvite = (originServerUrl: string): string | null => {
    const backendsStore = useSyncBackendsStore()
    const userServerUrl = backendsStore.backends[0]?.homeServerUrl
    if (!userServerUrl) return null
    if (normalizeUrl(originServerUrl) === normalizeUrl(userServerUrl)) return null
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
   * then tell the relay server to establish federation with the origin server.
   * Also creates a sync backend pointing to the relay server.
   */
  const setupFederationForSpaceAsync = async (
    relayServerUrl: string,
    originServerUrl: string,
    spaceId: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)

    // 1. Get relay server's DID and origin server's DID
    const relayServerDid = await getRelayServerDidAsync(relayServerUrl)
    const originServerDid = await getRelayServerDidAsync(originServerUrl)

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

    // 4. Tell relay server to establish federation with origin server
    const response = await fetchWithDidAuth(
      `${relayServerUrl}/federation/setup`,
      identity.privateKey,
      identity.did,
      'federation-setup',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ spaceId, originServerUrl, relayUcan }),
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
        homeServerUrl: relayServerUrl,
        spaceId,
        identityId: identity.id,
        enabled: true,
        priority: 0,
        type: 'relay',
        homeServerDid: relayServerDid,
        originServerDid,
      })
    }

    log.info(`Federation established: relay ${relayServerUrl} → origin ${originServerUrl} for space ${spaceId}`)
  }

  // =========================================================================
  // Capability Lookups
  // =========================================================================

  /** Get all capabilities the current user has for a given space */
  const getCapabilitiesForSpaceAsync = async (spaceId: string): Promise<string[]> => {
    const db = getDb()
    if (!db) return []

    const identityStore = useIdentityStore()
    const myDids = identityStore.ownIdentities.map(i => i.did)

    const tokens = await db.select()
      .from(haexUcanTokens)
      .where(eq(haexUcanTokens.spaceId, spaceId))

    return tokens
      .filter(t => myDids.includes(t.audienceDid) || myDids.includes(t.issuerDid))
      .map(t => t.capability)
  }

  /** Check if the current user has a specific capability (or space/admin) for a space */
  const hasCapabilityAsync = async (spaceId: string, capability: string): Promise<boolean> => {
    const capabilities = await getCapabilitiesForSpaceAsync(spaceId)
    return capabilities.includes(capability) || capabilities.includes('space/admin')
  }

  /**
   * Accept a local P2P invite. Tries all space endpoints until ClaimInvite succeeds.
   */
  const acceptLocalInviteAsync = async (invite: {
    id: string
    spaceId: string
    spaceName?: string | null
    spaceType?: string | null
    originUrl?: string | null
    spaceEndpoints: string | null
    tokenId: string | null
  }) => {
    if (!invite.spaceEndpoints || !invite.tokenId) {
      throw new Error('Missing invite data for local claim')
    }

    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const identity = identityStore.ownIdentities[0]
    if (!identity) throw new Error('No identity available')

    const endpoints: string[] = JSON.parse(invite.spaceEndpoints)
    if (endpoints.length === 0) throw new Error('No space endpoints in invite')

    let lastError: Error | null = null
    for (const endpointId of endpoints) {
      try {
        await invoke('local_delivery_claim_invite', {
          leaderEndpointId: endpointId,
          leaderRelayUrl: null,
          spaceId: invite.spaceId,
          tokenId: invite.tokenId,
          identityDid: identity.did,
          label: identity.label || null,
        })
        lastError = null
        break
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error))
        log.debug(`ClaimInvite to ${endpointId} failed: ${lastError.message}, trying next...`)
      }
    }
    if (lastError) throw lastError

    // Create or activate the space entry.
    // handle_push_invite does NOT create a dummy space (to avoid CRDT tombstone issues),
    // so we need to create it here on successful claim.
    const db = getDb()
    if (db) {
      const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, invite.spaceId)).limit(1)
      if (existing.length > 0) {
        await db.update(haexSpaces).set({ status: SpaceStatus.ACTIVE }).where(eq(haexSpaces.id, invite.spaceId))
      } else {
        await persistSpaceAsync({
          id: invite.spaceId,
          name: invite.spaceName || invite.spaceId.slice(0, 8),
          type: (invite.spaceType as SpaceTypeValue) || SpaceType.LOCAL,
          status: SpaceStatus.ACTIVE,
          serverUrl: invite.originUrl || '',
          createdAt: new Date().toISOString(),
        })
      }
    }

    await loadSpacesFromDbAsync()
    log.info(`Accepted local invite for space ${invite.spaceId}`)
  }

  /**
   * Queue a QUIC PushInvite without requiring leader mode.
   * Creates the invite token directly in the DB and queues outbox entries.
   * Used for online spaces where no QUIC leader is running.
   *
   * If tokenId is provided (e.g. the server inviteId), it is reused so
   * the receiver can later accept via the server using the same ID.
   */
  const queueQuicInviteAsync = async ({
    spaceId,
    tokenId,
    contactDid,
    contactEndpointIds,
    capabilities,
    includeHistory,
    expiresInSeconds,
  }: {
    spaceId: string
    tokenId?: string
    contactDid: string
    contactEndpointIds: string[]
    capabilities: string[]
    includeHistory: boolean
    expiresInSeconds: number
  }) => {
    const db = getDb()
    if (!db) throw new Error('No vault open')
    if (contactEndpointIds.length === 0) {
      throw new Error('Contact has no known EndpointId — share identities via QR code first')
    }

    const id = tokenId || crypto.randomUUID()
    const expiresAt = new Date(Date.now() + expiresInSeconds * 1000).toISOString()
    const now = new Date().toISOString()

    await db.insert(haexInviteTokens).values({
      id,
      spaceId,
      targetDid: contactDid,
      capabilities: JSON.stringify(capabilities),
      includeHistory,
      maxUses: 1,
      currentUses: 0,
      expiresAt,
      createdAt: now,
    })

    const { useInviteOutbox } = await import('@/composables/useInviteOutbox')
    const { createOutboxEntryAsync, processOutboxAsync } = useInviteOutbox()

    for (const endpointId of contactEndpointIds) {
      await createOutboxEntryAsync({
        spaceId,
        tokenId: id,
        targetDid: contactDid,
        targetEndpointId: endpointId,
        expiresAt,
      })
    }

    log.info(`Queued QUIC invite for ${contactDid} in space ${spaceId} (${contactEndpointIds.length} endpoint(s))`)

    // Trigger immediate outbox processing so the invite is sent right away
    // instead of waiting for the next 30s orchestrator tick
    processOutboxAsync().catch(err =>
      log.warn(`Immediate outbox processing failed (will retry): ${err}`),
    )
  }

  const clearCache = () => {
    spaces.value = []
  }

  return {
    spaces,
    visibleSpaces,
    activeSpaces,
    pendingSpaces,
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
    getCapabilitiesForSpaceAsync,
    hasCapabilityAsync,
    queueQuicInviteAsync,
    acceptLocalInviteAsync,
    persistSpaceAsync,
    startLocalSpaceLeadersAsync,
    removeSpaceFromDbAsync,
    clearCache,
  }
})
