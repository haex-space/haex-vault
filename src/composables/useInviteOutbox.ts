import { eq, and, lte } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexInviteOutbox, haexInviteTokens, haexPendingInvites, haexSpaces, haexSpaceDevices, haexUcanTokens } from '~/database/schemas'
import { OutboxStatus, SpaceStatus } from '~/database/constants'
import { createLogger } from '@/stores/logging'

const log = createLogger('INVITE-OUTBOX')

const BACKOFF_SECONDS = [0, 60, 300, 900, 3600] // immediate, 1m, 5m, 15m, 1h

function nextRetryDelay(retryCount: number): number {
  const seconds = BACKOFF_SECONDS[Math.min(retryCount, BACKOFF_SECONDS.length - 1)]!
  return seconds * 1000
}

export function useInviteOutbox() {
  const { currentVault } = storeToRefs(useVaultStore())
  const getDb = () => currentVault.value?.drizzle

  /** Create a new outbox entry for a PushInvite delivery. */
  const createOutboxEntryAsync = async (entry: {
    spaceId: string
    tokenId: string
    targetDid: string
    targetEndpointId: string
    expiresAt: string
  }) => {
    const db = getDb()
    if (!db) return

    const now = new Date().toISOString()
    await db.insert(haexInviteOutbox).values({
      id: crypto.randomUUID(),
      spaceId: entry.spaceId,
      tokenId: entry.tokenId,
      targetDid: entry.targetDid,
      targetEndpointId: entry.targetEndpointId,
      status: OutboxStatus.PENDING,
      retryCount: 0,
      nextRetryAt: now,
      expiresAt: entry.expiresAt,
      createdAt: now,
    })
  }

  /**
   * Process all pending outbox entries that are ready for retry.
   * Called periodically by the sync orchestrator.
   */
  const processOutboxAsync = async () => {
    const db = getDb()
    if (!db) return

    const now = new Date().toISOString()

    const entries = await db
      .select()
      .from(haexInviteOutbox)
      .where(
        and(
          eq(haexInviteOutbox.status, OutboxStatus.PENDING),
          lte(haexInviteOutbox.nextRetryAt, now),
        ),
      )

    if (entries.length === 0) return

    // Skip entries targeting our own endpoint (prevents self-invites via CRDT-synced outbox)
    const peerStore = usePeerStorageStore()
    const ownEndpointId = peerStore.nodeId

    // Load identities for inviterDid resolution
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    if (identityStore.ownIdentities.length === 0) {
      log.warn('No identity available for outbox processing')
      return
    }

    for (const entry of entries) {
      // Check if expired
      if (entry.expiresAt && entry.expiresAt <= now) {
        await db
          .update(haexInviteOutbox)
          .set({ status: OutboxStatus.EXPIRED })
          .where(eq(haexInviteOutbox.id, entry.id))
        // Delete token if expired > 2 weeks (keep for UI display)
        const twoWeeksMs = 14 * 24 * 60 * 60 * 1000
        if (Date.now() - new Date(entry.expiresAt!).getTime() > twoWeeksMs) {
          await db.delete(haexInviteTokens).where(eq(haexInviteTokens.id, entry.tokenId))
          log.info(`Deleted stale invite token ${entry.tokenId} (expired > 2 weeks)`)
        }
        log.info(`Outbox entry ${entry.id} expired`)
        continue
      }

      // Skip if targeting our own endpoint
      if (ownEndpointId && entry.targetEndpointId === ownEndpointId) {
        log.debug(`Outbox entry ${entry.id}: target is own endpoint, marking delivered`)
        await db
          .update(haexInviteOutbox)
          .set({ status: OutboxStatus.DELIVERED })
          .where(eq(haexInviteOutbox.id, entry.id))
        continue
      }

      // Load space info
      const spaceRows = await db
        .select()
        .from(haexSpaces)
        .where(eq(haexSpaces.id, entry.spaceId))
        .limit(1)
      const space = spaceRows[0]
      if (!space) {
        log.warn(`Outbox entry ${entry.id}: space ${entry.spaceId} not found`)
        continue
      }

      // Load invite token for capabilities and history flag
      const tokenRows = await db
        .select()
        .from(haexInviteTokens)
        .where(eq(haexInviteTokens.id, entry.tokenId))
        .limit(1)
      const token = tokenRows[0]
      if (!token || !token.capabilities) {
        log.warn(`Outbox entry ${entry.id}: invite token ${entry.tokenId} not found or has no capabilities, skipping`)
        await db
          .update(haexInviteOutbox)
          .set({ status: OutboxStatus.EXPIRED })
          .where(eq(haexInviteOutbox.id, entry.id))
        continue
      }

      // Token expired — mark outbox entry, clean up token after 2 weeks
      if (token.expiresAt && token.expiresAt <= now) {
        await db
          .update(haexInviteOutbox)
          .set({ status: OutboxStatus.EXPIRED })
          .where(eq(haexInviteOutbox.id, entry.id))
        const twoWeeksMs = 14 * 24 * 60 * 60 * 1000
        if (Date.now() - new Date(token.expiresAt!).getTime() > twoWeeksMs) {
          await db.delete(haexInviteTokens).where(eq(haexInviteTokens.id, token.id))
          log.info(`Deleted stale invite token ${token.id} (expired > 2 weeks)`)
        }
        continue
      }

      const capabilities: string[] = JSON.parse(token.capabilities)

      // Resolve the identity that issued UCANs for this space (the admin)
      const ucanRows = await db
        .select({ issuerDid: haexUcanTokens.issuerDid })
        .from(haexUcanTokens)
        .where(eq(haexUcanTokens.spaceId, entry.spaceId))
        .limit(1)
      const identity = ucanRows[0]
        ? identityStore.ownIdentities.find(id => id.did === ucanRows[0]!.issuerDid)
        : undefined
      if (!identity) {
        log.warn(`Outbox entry ${entry.id}: no identity found for space ${entry.spaceId}, using first available`)
      }
      const inviterIdentity = identity ?? identityStore.ownIdentities[0]!

      // Load all space device endpoints
      const devices = await db
        .select()
        .from(haexSpaceDevices)
        .where(eq(haexSpaceDevices.spaceId, entry.spaceId))
      const spaceEndpoints = devices.map(d => d.deviceEndpointId)

      try {
        const accepted = await invoke<boolean>('local_delivery_push_invite', {
          targetEndpointId: entry.targetEndpointId,
          spaceId: entry.spaceId,
          spaceName: space.name,
          spaceType: space.type,
          tokenId: entry.tokenId,
          capabilities,
          includeHistory: token.includeHistory ?? false,
          inviterDid: inviterIdentity.did,
          inviterLabel: inviterIdentity.label || null,
          spaceEndpoints,
          originUrl: space.originUrl || null,
          expiresAt: entry.expiresAt,
        })

        if (accepted) {
          await db
            .update(haexInviteOutbox)
            .set({ status: OutboxStatus.DELIVERED })
            .where(eq(haexInviteOutbox.id, entry.id))
          log.info(`Outbox entry ${entry.id} delivered successfully`)
        }
      } catch (error) {
        const nextCount = entry.retryCount + 1
        const delay = nextRetryDelay(nextCount)
        const nextRetry = new Date(Date.now() + delay).toISOString()

        await db
          .update(haexInviteOutbox)
          .set({
            retryCount: nextCount,
            nextRetryAt: nextRetry,
          })
          .where(eq(haexInviteOutbox.id, entry.id))

        log.debug(
          `Outbox entry ${entry.id} retry #${nextCount}, next at ${nextRetry}: ${error}`,
        )
      }
    }
  }

  /**
   * Clean up old responded invites via CRDT delete.
   * Safe because haex_pending_invites rows have unique UUIDs — tombstones
   * won't collide with any row on the sender's device.
   * The CRDT purge mechanism handles tombstone cleanup.
   *
   * Note: haex_spaces entries with status='declined' are NOT deleted here
   * because their primary key (space ID) matches the sender's active space.
   * A CRDT tombstone for that ID would destroy the sender's space.
   * These rows are tiny and filtered out by the UI.
   */
  const cleanupOldInvitesAsync = async () => {
    const db = getDb()
    if (!db) return

    const sevenDaysAgo = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString()

    await db.delete(haexPendingInvites).where(
      and(
        lte(haexPendingInvites.respondedAt, sevenDaysAgo),
        lte(haexPendingInvites.createdAt, sevenDaysAgo),
      ),
    )
  }

  return { createOutboxEntryAsync, processOutboxAsync, cleanupOldInvitesAsync }
}
