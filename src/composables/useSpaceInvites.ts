import { eq } from 'drizzle-orm'
import { listen } from '@tauri-apps/api/event'
import {
  haexPendingInvites,
  type SelectHaexPendingInvites,
} from '~/database/schemas'
import type { SpaceWithType } from '@/stores/spaces'
import { SpaceType, SpaceStatus } from '~/database/constants'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { useInvitePolicy } from '@/composables/useInvitePolicy'
import { useMlsDelivery } from '@/composables/useMlsDelivery'
import { useCurrentIdentity } from '@/composables/useCurrentIdentity'

export type InvitePolicyValue = 'all' | 'contacts_only' | 'nobody'

/**
 * Encapsulates the pending-invite domain for the Spaces settings view:
 * policy persistence, accept/decline flows (QUIC vs server), and live
 * reloading on push-invite events.
 *
 * UI concerns (toast, i18n) live in the consumer — this composable throws
 * on failure and exposes state refs the component can bind to.
 */
export function useSpaceInvites() {
  const { currentVault } = storeToRefs(useVaultStore())
  const spacesStore = useSpacesStore()
  const identityStore = useIdentityStore()
  const syncBackendsStore = useSyncBackendsStore()
  const { backends: syncBackends } = storeToRefs(syncBackendsStore)
  const { setPolicy, getPolicy } = useInvitePolicy()
  const { ensureCurrentIdentityAsync } = useCurrentIdentity()

  const pendingInvites = ref<SelectHaexPendingInvites[]>([])
  const currentPolicy = ref<InvitePolicyValue>('contacts_only')

  const getDb = () => currentVault.value?.drizzle

  const loadInvitesAsync = async () => {
    const db = getDb()
    if (!db) return

    const rows = await db
      .select()
      .from(haexPendingInvites)
      .where(eq(haexPendingInvites.status, 'pending'))

    await identityStore.loadIdentitiesAsync()
    pendingInvites.value = rows.map((row) => {
      const knownIdentity = identityStore.identities.find(i => i.did === row.inviterDid)
      if (!knownIdentity) return row
      return {
        ...row,
        inviterLabel: knownIdentity.name,
        inviterAvatar: knownIdentity.avatar,
        inviterAvatarOptions: knownIdentity.avatarOptions,
      }
    })
    currentPolicy.value = await getPolicy()
  }

  const changePolicyAsync = async (policy: InvitePolicyValue) => {
    await setPolicy(policy)
    currentPolicy.value = policy
  }

  const getServerUrlForSpace = (spaceId: string): string | undefined => {
    const backend = syncBackends.value.find((b) => b.spaceId === spaceId)
    return backend?.homeServerUrl
  }

  /**
   * Accepts a pending invite. Picks the best acceptance path automatically:
   * - QUIC ClaimInvite when the invite carries space endpoints (pushed via P2P)
   * - Server-based accept + local space persistence otherwise
   *
   * Persists the error to the invite row on failure so it's diagnosable later,
   * then rethrows so the caller can surface a toast.
   */
  const acceptInviteAsync = async (invite: SelectHaexPendingInvites) => {
    const db = getDb()
    try {
      const originUrl =
        invite.originUrl || getServerUrlForSpace(invite.spaceId)
      const endpoints: string[] = invite.spaceEndpoints
        ? JSON.parse(invite.spaceEndpoints)
        : []

      if (endpoints.length > 0) {
        // QUIC invite — accept via ClaimInvite to one of the space endpoints
        // (acceptLocalInviteAsync creates the real space entry on success)
        await spacesStore.acceptLocalInviteAsync(invite)
      } else if (originUrl && invite.tokenId) {
        // Online space without QUIC endpoints — accept via server
        const identity = await ensureCurrentIdentityAsync()
        const ownerIdentity = await identityStore.ensureIdentityForDidAsync(invite.inviterDid, {
          name: invite.inviterLabel,
          avatar: invite.inviterAvatar,
          avatarOptions: invite.inviterAvatarOptions,
          source: 'space',
        })
        const delivery = useMlsDelivery(originUrl, invite.spaceId, {
          privateKey: identity.privateKey,
          did: identity.did,
        })
        await delivery.acceptInviteAsync(invite.tokenId)

        await spacesStore.persistSpaceAsync({
          id: invite.spaceId,
          name: invite.spaceName || invite.spaceId.slice(0, 8),
          type:
            (invite.spaceType as SpaceWithType['type']) || SpaceType.ONLINE,
          status: SpaceStatus.ACTIVE,
          ownerIdentityId: ownerIdentity.id,
          originUrl: originUrl,
          createdAt: new Date().toISOString(),
          capabilities: [],
        })
        await spacesStore.loadSpacesFromDbAsync()

        // Add self as space member (non-fatal) — reuses the identity resolved
        // for MlsDelivery above so we don't re-read the store.
        await spacesStore.addSelfAsSpaceMemberAsync(invite.spaceId, identity, 'read')
      } else {
        throw new Error('No server URL or endpoints available for invite')
      }

      if (db) {
        await db
          .update(haexPendingInvites)
          .set({
            status: 'accepted',
            respondedAt: new Date().toISOString(),
          })
          .where(eq(haexPendingInvites.id, invite.id))
      }

      await loadInvitesAsync()
    } catch (error) {
      // Persist error to pending invite so failures are diagnosable
      if (db) {
        const errorMessage =
          error instanceof Error ? error.message : String(error)
        await db
          .update(haexPendingInvites)
          .set({
            status: `error:${errorMessage.slice(0, 200)}`,
            respondedAt: new Date().toISOString(),
          })
          .where(eq(haexPendingInvites.id, invite.id))
          .catch(() => {})
      }
      throw error
    }
  }

  /**
   * Declines a pending invite. Server-side decline is best-effort — a failed
   * network request does not block the local delete.
   */
  const declineInviteAsync = async (invite: SelectHaexPendingInvites) => {
    const originUrl =
      invite.originUrl || getServerUrlForSpace(invite.spaceId)

    if (originUrl && invite.tokenId) {
      try {
        const identity = await ensureCurrentIdentityAsync()
        await fetchWithDidAuth(
          `${originUrl}/spaces/${invite.spaceId}/invites/${invite.tokenId}/decline`,
          identity.privateKey,
          identity.did,
          'decline-invite',
          { method: 'POST', headers: { 'Content-Type': 'application/json' } },
        )
      } catch {
        // Server decline is best-effort — invite will expire on server
      }
    }

    // CRDT delete is safe — haex_pending_invites rows have unique UUIDs
    // that don't collide with any row on the sender's device.
    const db = getDb()
    if (db) {
      await db
        .delete(haexPendingInvites)
        .where(eq(haexPendingInvites.id, invite.id))
    }

    await loadInvitesAsync()
  }

  /**
   * Registers a tauri event listener that refreshes the local invite list
   * whenever a push invite arrives while the settings page is open. Returns
   * an unregister function to be called in `onUnmounted`.
   */
  const listenForPushInvitesAsync = async (): Promise<() => void> => {
    return listen('push-invite-received', async () => {
      await loadInvitesAsync()
    })
  }

  return {
    pendingInvites,
    currentPolicy,
    loadInvitesAsync,
    changePolicyAsync,
    acceptInviteAsync,
    declineInviteAsync,
    listenForPushInvitesAsync,
  }
}
