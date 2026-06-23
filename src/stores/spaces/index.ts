import { eq } from 'drizzle-orm'
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import { haexSpaces } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { NoCurrentIdentityError } from '@/composables/useCurrentIdentity'
import { requireDb } from '~/stores/vault'
import { SpaceType, SpaceStatus } from '~/database/constants'
import spacesDe from './spaces.de.json'
import spacesEn from './spaces.en.json'
import type { ResolvedIdentity, SpaceWithType } from './types'
export type { ResolvedIdentity, SpaceWithType } from './types'
import { rowToSpace } from './types'
import {
  loadMemberSpaceIds,
  loadSpacesFromDb,
  persistSpace,
  removeSpaceFromDb,
} from './persistence'
import {
  startLocalSpaceLeaders,
  startLocalSpacePeerSync,
  startPeerSyncForLocalSpace,
} from './peerSync'
import { ensureDefaultSpace, ensureVaultSpace } from './bootstrap'
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
  // Set of spaceIds where any of the user's own identities is a member
  // (joined `haex_space_members` against `ownIdentities`). Populated by
  // `loadMemberSpaceIdsAsync` and refreshed alongside `loadSpacesFromDbAsync`.
  // Used by `visibleSpaces` to drop phantom space rows that arrive via CRDT
  // sync when a peer's foreign-space row pulls cross-references along with
  // it — those rows must never reach the UI because the local user has no
  // membership claim on them.
  const memberSpaceIds = ref<Set<string>>(new Set())
  const db = computed(() => currentVault.value?.drizzle)
  const visibleSpaces = computed(() => {
    const identityStore = useIdentityStore()
    const ownIdentityIds = new Set(identityStore.ownIdentities.map((i) => i.id))
    return spaces.value
      .filter((s) => {
        if (s.type === SpaceType.VAULT) return false
        // Pending invites: shown so the user can accept them, even though
        // membership has not been recorded yet.
        if (s.status === SpaceStatus.PENDING) return true
        // Owner of the space — guard against `addSelfAsSpaceMember` failing
        // (it is intentionally non-fatal at creation time) so creators
        // always see their own space.
        if (ownIdentityIds.has(s.ownerIdentityId)) return true
        // Membership cross-check: must have a `haex_space_members` row for
        // one of our own identities.
        return memberSpaceIds.value.has(s.id)
      })
      .map(rowToSpace)
  })
  const activeSpaces = computed(() =>
    visibleSpaces.value.filter((s) => s.status === SpaceStatus.ACTIVE),
  )
  const pendingSpaces = computed(() =>
    visibleSpaces.value.filter((s) => s.status === SpaceStatus.PENDING),
  )

  // Spaces the user belongs to but does NOT own — the only ones where a new
  // device meaningfully publishes its endpoint. Owned spaces (personal/default
  // + self-created) need no publishing: the owner's endpoints are already known.
  // Pending (unaccepted) invites don't count — there's no membership yet.
  //
  // Guard against an empty identity store: without any own identities loaded,
  // every space would falsely look 'foreign' (ownerIdentityId never matches),
  // which would pop the publishing dialog for the user's own personal space
  // during a brief hydration race. Treat that state as "unknown → no foreign
  // spaces"; once identities load, the computed re-runs naturally.
  const foreignSpaces = computed(() => {
    const identityStore = useIdentityStore()
    if (identityStore.ownIdentities.length === 0) return []
    const ownIdentityIds = new Set(identityStore.ownIdentities.map((i) => i.id))
    return visibleSpaces.value.filter(
      (s) => s.status !== SpaceStatus.PENDING && !ownIdentityIds.has(s.ownerIdentityId),
    )
  })

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
  // Persistence (thin wrappers around ./persistence)
  // =========================================================================

  const loadMemberSpaceIdsAsync = async () => {
    const identityStore = useIdentityStore()
    const ownIds = identityStore.ownIdentities.map((i) => i.id)
    await loadMemberSpaceIds(db.value, ownIds, memberSpaceIds)
  }

  const loadSpacesFromDbAsync = async () =>
    loadSpacesFromDb(db.value, spaces, loadMemberSpaceIdsAsync)

  const persistSpaceAsync = async (space: SpaceWithType) =>
    persistSpace(db.value, space, loadSpacesFromDbAsync)

  const removeSpaceFromDbAsync = async (spaceId: string) =>
    removeSpaceFromDb(db.value, spaces, spaceId)

  // =========================================================================
  // Startup (thin wrappers around ./peerSync + ./bootstrap)
  // =========================================================================

  const startLocalSpaceLeadersAsync = () =>
    startLocalSpaceLeaders(spaces.value)

  const startPeerSyncForLocalSpaceAsync = startPeerSyncForLocalSpace

  const startLocalSpacePeerSyncAsync = async () => {
    const identityStore = useIdentityStore()
    const myIdentity = identityStore.ownIdentities[0]
    await startLocalSpacePeerSync(spaces.value, myIdentity?.did)
  }

  const retryPendingWelcomesAsync = async () => {
    try {
      await retryPendingWelcomes(requireDb())
    } catch (error) {
      log.warn(`Pending welcome recovery failed: ${error}`)
    }
  }

  const ensureVaultSpaceAsync = async (vaultId: string, vaultName: string) => {
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const ownerIdentity = identityStore.ownIdentities[0]
    return ensureVaultSpace(db.value, vaultId, vaultName, ownerIdentity?.id)
  }

  const ensureDefaultSpaceAsync = async () => {
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const name = $i18n.t('spaces.defaultSpaceName')
    return ensureDefaultSpace(
      db.value,
      spaces.value,
      name,
      identityStore.ownIdentities[0]?.id,
      loadSpacesFromDbAsync,
      createLocalSpaceAsync,
    )
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
      // Local-only leave: drop UCANs immediately. The previous policy kept
      // them around so the LEAVING-state sync loop could push the membership
      // delete to the leader, but in practice that left stale tokens lying
      // around for up to 30 days (LEAVE_GIVE_UP_AFTER_MS) — and on a re-invite
      // before that timeout, the new and the old UCAN coexisted under the
      // same (space_id, audience_did) with potentially different capabilities.
      // We accept the trade-off: the leader may not see this device's leave
      // immediately, but the leader's own membership/MLS-removal flow is the
      // authoritative side anyway. The space row stays LEAVING so the cleanup
      // pass can finalize and drop it together with anything else hanging off.
      const d = requireDb()
      await d
        .update(haexSpaces)
        .set({
          status: SpaceStatus.LEAVING,
          modifiedAt: new Date().toISOString(),
        })
        .where(eq(haexSpaces.id, spaceId))
      await removeSelfFromSpace(requireDb(), spaceId, ownIdentityIds, {
        deleteUcans: true,
      })
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
    foreignSpaces,
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
