import type { DecryptedSpace } from '@haex-space/vault-sdk'
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import { eq } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaces } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import type { ElectionResultInfo } from '@bindings/ElectionResultInfo'
import { createLogger } from '@/stores/logging'
import { NoCurrentIdentityError } from '@/composables/useCurrentIdentity'
import { requireDb } from '~/stores/vault'
import { SpaceType, SpaceStatus } from '~/database/constants'
import type {
  SpaceType as SpaceTypeValue,
  SpaceStatus as SpaceStatusValue,
} from '~/database/constants'
import spacesDe from './spaces.de.json'
import spacesEn from './spaces.en.json'

// Module imports
import {
  addMemberToSpace,
  addSelfAsSpaceMember,
  getSpaceMembers,
  updateOwnSpaceProfile,
  getMemberPublicKeysForSpace,
  removeSelfFromSpace,
  removeSpaceMember,
  migrateExistingMembers,
} from './members'
import { getCapabilitiesForSpace, hasCapability } from './capabilities'
import { setupFederationForSpace } from './federation'
import {
  reconcileMlsAfterMemberSyncAsync,
  resetMemberSnapshots,
} from './reconcileMls'
import {
  inviteMember,
  createInviteToken,
  buildInviteLink,
  claimInviteToken,
  finalizeInvite,
  processWelcomes,
  retryPendingWelcomes,
  acceptLocalInvite,
  queueQuicInvite,
} from './invites'
import {
  createLocalSpace,
  createOnlineSpace,
  updateSpaceName,
  migrateSpaceServer,
  listSpaces,
  leaveSpace,
  deleteSpace,
  cleanupCompletedLeavesAsync,
  removeIdentityFromSpace,
} from './crud'

/** Extended space type including the DB type field (vault/online/local) */
export interface SpaceWithType extends DecryptedSpace {
  type: SpaceTypeValue
  status: SpaceStatusValue
  ownerIdentityId: string
}

export interface ResolvedIdentity {
  id: string
  publicKey: string
  privateKey: string
  did: string
  name: string
}

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const { $i18n } = useNuxtApp()
  $i18n.mergeLocaleMessage('de', { spaces: spacesDe })
  $i18n.mergeLocaleMessage('en', { spaces: spacesEn })

  const { currentVault } = storeToRefs(useVaultStore())

  // =========================================================================
  // State
  // =========================================================================

  const spaces = ref<SelectHaexSpaces[]>([])
  const db = computed(() => currentVault.value?.drizzle)
  const rowToSpace = (row: SelectHaexSpaces): SpaceWithType => ({
    id: row.id,
    name: row.name,
    type: (row.type as SpaceTypeValue) ?? SpaceType.ONLINE,
    status: (row.status as SpaceStatusValue) ?? SpaceStatus.ACTIVE,
    ownerIdentityId: row.ownerIdentityId,
    originUrl: row.originUrl ?? '',
    createdAt: row.createdAt ?? '',
    capabilities: [],
  })
  const visibleSpaces = computed(() =>
    spaces.value
      .filter((s) => s.type !== SpaceType.VAULT)
      .map(rowToSpace),
  )
  const activeSpaces = computed(() =>
    visibleSpaces.value.filter((s) => s.status === SpaceStatus.ACTIVE),
  )
  const pendingSpaces = computed(() =>
    visibleSpaces.value.filter((s) => s.status === SpaceStatus.PENDING),
  )

  // =========================================================================
  // Internal helpers
  // =========================================================================

  const resolveIdentityAsync = async (
    identityId: string,
  ): Promise<ResolvedIdentity> => {
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityByIdAsync(identityId)
    if (!identity?.privateKey)
      throw new Error(`Identity ${identityId} not found or has no private key`)
    return {
      id: identity.id,
      publicKey: await didKeyToPublicKeyAsync(identity.did),
      privateKey: identity.privateKey,
      did: identity.did,
      name: identity.name,
    }
  }

  // =========================================================================
  // Persistence
  // =========================================================================

  const persistSpaceAsync = async (space: SpaceWithType) => {
    const d = db.value
    if (!d) return

    const existing = await d
      .select()
      .from(haexSpaces)
      .where(eq(haexSpaces.id, space.id))
      .limit(1)

    if (existing.length > 0) {
      await d
        .update(haexSpaces)
        .set({
          name: space.name,
          ownerIdentityId: space.ownerIdentityId,
          originUrl: space.originUrl || null,
          status: space.status,
          modifiedAt: new Date().toISOString(),
        })
        .where(eq(haexSpaces.id, space.id))
    } else {
      await d.insert(haexSpaces).values({
        id: space.id,
        type: space.type,
        name: space.name,
        ownerIdentityId: space.ownerIdentityId,
        originUrl: space.originUrl || null,
        status: space.status,
      })
    }

    await loadSpacesFromDbAsync()
  }

  const removeSpaceFromDbAsync = async (spaceId: string) => {
    const d = db.value
    if (d) {
      await d.delete(haexSpaces).where(eq(haexSpaces.id, spaceId))
    }
    spaces.value = spaces.value.filter((s) => s.id !== spaceId)
  }

  const loadSpacesFromDbAsync = async () => {
    const d = db.value
    if (!d) return

    spaces.value = await d.select().from(haexSpaces)

    return spaces.value
  }

  // =========================================================================
  // Startup
  // =========================================================================

  const startLocalSpaceLeadersAsync = async () => {
    for (const space of spaces.value) {
      if (
        space.type === SpaceType.LOCAL &&
        space.status === SpaceStatus.ACTIVE
      ) {
        try {
          await invoke('local_delivery_start', {
            spaceId: space.id,
          })
          log.info(
            `Started leader mode for local space ${space.id}`,
          )
        } catch {
          // Already running — ignore
        }
      }
    }
  }

  /**
   * Start a peer sync loop for a single local space after running leader
   * election. If a `hintLeaderEndpointId` is provided and election does not
   * find a leader, falls back to the hint — useful right after an invite
   * Accept, where we know which endpoint served the ClaimInvite but election
   * may not yet have the fresh space devices registered.
   */
  const startPeerSyncForLocalSpaceAsync = async (
    spaceId: string,
    identityDid: string,
    hintLeaderEndpointId?: string,
    hintLeaderRelayUrl?: string | null,
  ): Promise<void> => {
    let leaderEndpointId: string | undefined
    let leaderRelayUrl: string | null | undefined
    try {
      const election = await invoke<ElectionResultInfo>(
        'local_delivery_elect',
        { spaceId },
      )
      if (election.role === 'leader') {
        log.debug(`Space ${spaceId}: self is leader, no peer sync needed`)
        return
      }
      if (election.role === 'peer' && election.leaderEndpointId) {
        leaderEndpointId = election.leaderEndpointId
        leaderRelayUrl = election.leaderRelayUrl
      } else {
        log.debug(`Space ${spaceId}: no leader found via election (role=${election.role})`)
      }
    } catch (error) {
      log.warn(`Election for space ${spaceId} failed: ${error}`)
    }

    if (!leaderEndpointId && hintLeaderEndpointId) {
      leaderEndpointId = hintLeaderEndpointId
      leaderRelayUrl = hintLeaderRelayUrl ?? null
      log.info(`Space ${spaceId}: using hint endpoint as leader (${hintLeaderEndpointId.slice(0, 16)})`)
    }

    if (!leaderEndpointId) return

    // UCAN is resolved inside Rust from haex_ucan_tokens at connect/reconnect
    // time — no token to pass from the frontend.
    try {
      await invoke('local_delivery_connect', {
        spaceId,
        leaderEndpointId,
        leaderRelayUrl: leaderRelayUrl ?? null,
        identityDid,
      })
      log.info(`Started peer sync for space ${spaceId} → leader ${leaderEndpointId.slice(0, 16)}`)
    } catch (error) {
      // Already connected, or temporarily unreachable — non-fatal.
      log.debug(`Peer sync connect for ${spaceId}: ${error}`)
    }
  }

  /**
   * For every joined local space, run leader election and — if another
   * device is the elected leader — start a peer sync loop against them.
   *
   * Without this, an invitee-side vault accepts the MLS welcome but never
   * pulls CRDT history (peer_shares, other members, space_devices), so the
   * space appears mostly empty after joining.
   *
   * Idempotent: `local_delivery_connect` errors if a loop is already
   * running for the space — we swallow that case.
   */
  const startLocalSpacePeerSyncAsync = async () => {
    const identityStore = useIdentityStore()
    const myIdentity = identityStore.ownIdentities[0]
    if (!myIdentity) {
      log.warn('Peer sync skipped: no own identity')
      return
    }

    for (const space of spaces.value) {
      // ACTIVE spaces sync normally. LEAVING spaces also need peer-sync
      // running so their pending delete-log entries can be pushed to the
      // leader the next time it is reachable; without this the offline-leave
      // resilience would never have a transport to flush over.
      const wantsPeerSync =
        space.type === SpaceType.LOCAL &&
        (space.status === SpaceStatus.ACTIVE
          || space.status === SpaceStatus.LEAVING)
      if (!wantsPeerSync) continue

      await startPeerSyncForLocalSpaceAsync(
        space.id,
        myIdentity.did,
      ).catch((error) => {
        log.warn(`Peer sync for space ${space.id} failed: ${error}`)
      })
    }
  }

  const retryPendingWelcomesAsync = async () => {
    try {
      await retryPendingWelcomes(requireDb())
    } catch (error) {
      log.warn(`Pending welcome recovery failed: ${error}`)
    }
  }

  const ensureVaultSpaceAsync = async (vaultId: string, vaultName: string) => {
    const d = db.value
    if (!d) {
      log.error('ensureVaultSpaceAsync: no DB available')
      return
    }

    const existing = await d
      .select()
      .from(haexSpaces)
      .where(eq(haexSpaces.id, vaultId))
      .limit(1)
    if (existing.length > 0) {
      log.info(`Vault space ${vaultId} already exists`)
      return
    }

    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const ownerIdentity = identityStore.ownIdentities[0]
    if (!ownerIdentity) {
      throw new Error('Cannot create vault space without an identity')
    }

    await d.insert(haexSpaces).values({
      id: vaultId,
      type: SpaceType.VAULT,
      name: vaultName,
      ownerIdentityId: ownerIdentity.id,
      originUrl: '',
    })
    log.info(`Created vault space "${vaultName}" (${vaultId})`)
  }

  const ensureDefaultSpaceAsync = async () => {
    const d = db.value
    if (!d) return

    const localSpaces = await d
      .select()
      .from(haexSpaces)
      .where(eq(haexSpaces.type, SpaceType.LOCAL))
      .limit(1)

    if (localSpaces.length > 0) {
      if (!spaces.value.find((s) => s.id === localSpaces[0]!.id)) {
        await loadSpacesFromDbAsync()
      }
      return
    }

    const name = $i18n.t('spaces.defaultSpaceName')
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const vaultOwnerId = spaces.value.find(
      (s) => s.type === SpaceType.VAULT,
    )?.ownerIdentityId
    const defaultOwnerId = vaultOwnerId || identityStore.ownIdentities[0]?.id
    if (!defaultOwnerId) {
      throw new Error('No identity available for default space')
    }
    await createLocalSpaceAsync(name, defaultOwnerId)
    log.info(`Default space "${name}" created`)
  }

  // =========================================================================
  // Delegating wrappers — thin functions that pass state to module functions
  // =========================================================================

  const createLocalSpaceAsync = (
    spaceName: string,
    ownerIdentityId: string,
    spaceId?: string,
  ) =>
    createLocalSpace(
      requireDb(),
      spaceName,
      ownerIdentityId,
      persistSpaceAsync,
      spaceId,
    )

  const createSpaceAsync = async (
    originUrl: string,
    spaceName: string,
    selfLabel: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    return createOnlineSpace(
      requireDb(),
      originUrl,
      spaceName,
      selfLabel,
      identity,
      persistSpaceAsync,
      async () => {
        await listSpacesAsync(originUrl, identityId)
      },
    )
  }

  const updateSpaceNameAsync = (spaceId: string, newName: string) =>
    updateSpaceName(activeSpaces.value, spaceId, newName, persistSpaceAsync)

  const migrateSpaceServerAsync = async (
    spaceId: string,
    oldServerUrl: string,
    newServerUrl: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    return migrateSpaceServer(
      activeSpaces.value,
      spaceId,
      oldServerUrl,
      newServerUrl,
      identity,
      persistSpaceAsync,
    )
  }

  const listSpacesAsync = async (originUrl: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return listSpaces(identity, originUrl, persistSpaceAsync)
  }

  const leaveSpaceAsync = async (
    originUrl: string,
    spaceId: string,
    identityId: string | null,
  ) => {
    const space = activeSpaces.value.find((s) => s.id === spaceId)
    const isLocalLeave = space?.type === SpaceType.LOCAL || !originUrl

    // Validate the remote-leave precondition first, BEFORE any destructive
    // local mutations. If we threw NoCurrentIdentityError after deleting
    // membership/UCAN rows the local DB would be half-mutated with no way
    // to retry the remote DELETE.
    if (!isLocalLeave && !identityId) {
      throw new NoCurrentIdentityError()
    }

    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const ownIdentityIds = identityStore.ownIdentities.map((i) => i.id)

    if (isLocalLeave) {
      // Local-only leave order matters:
      //  1. Mark space LEAVING + bump modifiedAt — peer-sync loop must keep
      //     running so the upcoming membership delete can be pushed. The
      //     loop only processes ACTIVE | LEAVING rows.
      //  2. Delete membership row (BEFORE-DELETE trigger → haex_deleted_rows).
      //     UCAN tokens are kept here so the sync loop can still authenticate
      //     against the leader. They are removed later by
      //     `cleanupCompletedLeavesAsync` once propagation is confirmed.
      //  3. cleanup pass eventually removes haex_spaces row + UCANs.
      const d = requireDb()
      await d
        .update(haexSpaces)
        .set({
          status: SpaceStatus.LEAVING,
          modifiedAt: new Date().toISOString(),
        })
        .where(eq(haexSpaces.id, spaceId))
      await removeSelfFromSpace(requireDb(), spaceId, ownIdentityIds)
      // Reload reactive state so UI immediately stops showing the space.
      await loadSpacesFromDbAsync()
      log.info(`Marked local space ${spaceId} as LEAVING (push pending)`)
      return
    }

    // Remote leave: home server is online by definition of the call.
    // We can delete UCAN tokens immediately since the HTTP DELETE acks
    // synchronously and there's no offline-resilience window to keep
    // them alive for.
    await removeSelfFromSpace(requireDb(), spaceId, ownIdentityIds, {
      deleteUcans: true,
    })
    const identity = await resolveIdentityAsync(identityId!)
    return leaveSpace(identity, originUrl, spaceId, removeSpaceFromDbAsync)
  }

  const deleteSpaceAsync = (originUrl: string, spaceId: string) =>
    deleteSpace(activeSpaces.value, originUrl, spaceId, removeSpaceFromDbAsync)

  const removeIdentityFromSpaceAsync = (
    spaceId: string,
    identityPublicKey: string,
  ) =>
    removeIdentityFromSpace(
      requireDb(),
      activeSpaces.value,
      spaceId,
      identityPublicKey,
    )

  const removeSpaceMemberAsync = (spaceId: string, memberDid: string) =>
    removeSpaceMember(requireDb(), spaceId, memberDid)

  const inviteMemberAsync = async (
    originUrl: string,
    spaceId: string,
    inviteeDid: string,
    capability: string,
    identityId: string,
    includeHistory = false,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    return inviteMember(
      activeSpaces.value,
      originUrl,
      spaceId,
      inviteeDid,
      capability,
      identity,
      includeHistory,
    )
  }

  const createInviteTokenAsync = (
    originUrl: string,
    spaceId: string,
    options: {
      capability?: string
      maxUses?: number
      expiresInSeconds: number
      label?: string
    },
  ) => createInviteToken(activeSpaces.value, originUrl, spaceId, options)

  const claimInviteTokenAsync = async (
    originUrl: string,
    spaceId: string,
    tokenId: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    return claimInviteToken(
      requireDb(),
      originUrl,
      spaceId,
      tokenId,
      identity,
      persistSpaceAsync,
    )
  }

  const finalizeInviteAsync = async (
    originUrl: string,
    spaceId: string,
    inviteeDid: string,
    identityId: string,
    inviteId?: string,
    capability?: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    return finalizeInvite(
      originUrl,
      spaceId,
      inviteeDid,
      identity,
      inviteId,
      capability,
    )
  }

  const processWelcomesAsync = async (
    originUrl: string,
    spaceId: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    return processWelcomes(requireDb(), originUrl, spaceId, identity)
  }

  const acceptLocalInviteAsync = (
    invite: Parameters<typeof acceptLocalInvite>[1],
  ) =>
    acceptLocalInvite(
      requireDb(),
      invite,
      persistSpaceAsync,
      async () => {
        await loadSpacesFromDbAsync()
      },
      startPeerSyncForLocalSpaceAsync,
    )

  const queueQuicInviteAsync = (
    params: Parameters<typeof queueQuicInvite>[1],
  ) => queueQuicInvite(requireDb(), params)

  const setupFederationForSpaceAsync = async (
    relayServerUrl: string,
    originServerUrl: string,
    spaceId: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    return setupFederationForSpace(
      relayServerUrl,
      originServerUrl,
      spaceId,
      identity,
    )
  }

  const getCapabilitiesForSpaceAsync = async (spaceId: string) => {
    const d = db.value
    if (!d) return []
    const identityStore = useIdentityStore()
    return getCapabilitiesForSpace(
      d,
      spaceId,
      identityStore.ownIdentities.map((i) => i.did),
    )
  }

  const hasCapabilityAsync = async (spaceId: string, capability: string) => {
    const d = db.value
    if (!d) return false
    const identityStore = useIdentityStore()
    return hasCapability(
      d,
      spaceId,
      capability,
      identityStore.ownIdentities.map((i) => i.did),
    )
  }

  const addMemberToSpaceAsync = (
    params: Parameters<typeof addMemberToSpace>[1],
  ) => addMemberToSpace(requireDb(), params)

  const addSelfAsSpaceMemberAsync = (
    spaceId: string,
    identity: {
      did: string
      id: string
      avatar?: string | null
      avatarOptions?: string | null
    },
    role: string,
  ) => addSelfAsSpaceMember(requireDb(), spaceId, identity, role)

  const getSpaceMembersAsync = (spaceId: string) =>
    getSpaceMembers(requireDb(), spaceId)

  const updateOwnSpaceProfileAsync = (
    spaceId: string,
    profile: {
      name?: string
      avatar?: string | null
      avatarOptions?: string | null
    },
  ) => {
    const d = db.value
    if (!d) return
    const identityStore = useIdentityStore()
    return updateOwnSpaceProfile(
      d,
      identityStore.ownIdentities.map((i) => i.id),
      spaceId,
      profile,
    )
  }

  const getMemberPublicKeysForSpaceAsync = (spaceId: string) =>
    getMemberPublicKeysForSpace(requireDb(), spaceId)

  const migrateExistingMembersAsync = async () => {
    const d = db.value
    if (!d) return
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    return migrateExistingMembers(
      d,
      identityStore.identities.map(
        (i: {
          id: string
          did: string
        }) => ({
          id: i.id,
          did: i.did,
        }),
      ),
    )
  }

  const clearCache = () => {
    spaces.value = []
    // Drop per-space MLS-reconcile snapshots so a re-opened vault doesn't
    // diff against a previous vault's member set.
    resetMemberSnapshots()
  }

  const reconcileMlsForLocalSpacesAsync = async () => {
    const d = db.value
    if (!d) return
    await reconcileMlsAfterMemberSyncAsync(d, activeSpaces.value)
  }

  /**
   * Drops `haex_spaces` rows for departed-but-not-yet-cleaned LEAVING
   * spaces. Called on vault startup; safe to call repeatedly thanks to
   * the per-space age check inside.
   */
  const cleanupCompletedLeavesAsyncMethod = async () => {
    const d = db.value
    if (!d) return
    const removed = await cleanupCompletedLeavesAsync(d, removeSpaceFromDbAsync)
    if (removed > 0) {
      await loadSpacesFromDbAsync()
    }
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
    removeSpaceMemberAsync,
    setupFederationForSpaceAsync,
    getCapabilitiesForSpaceAsync,
    hasCapabilityAsync,
    addMemberToSpaceAsync,
    addSelfAsSpaceMemberAsync,
    getSpaceMembersAsync,
    updateOwnSpaceProfileAsync,
    getMemberPublicKeysForSpaceAsync,
    migrateExistingMembersAsync,
    queueQuicInviteAsync,
    acceptLocalInviteAsync,
    persistSpaceAsync,
    startLocalSpaceLeadersAsync,
    startLocalSpacePeerSyncAsync,
    startPeerSyncForLocalSpaceAsync,
    retryPendingWelcomesAsync,
    removeSpaceFromDbAsync,
    reconcileMlsForLocalSpacesAsync,
    cleanupCompletedLeavesAsync: cleanupCompletedLeavesAsyncMethod,
    clearCache,
  }
})
