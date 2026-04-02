import { eq, and, lte } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexInviteOutbox, haexInviteTokens, haexSpaces, haexSpaceDevices } from '~/database/schemas'
import { OutboxStatus } from '~/database/constants'
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

    // Load identity for inviterDid
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    const identity = identityStore.ownIdentities[0]
    if (!identity) {
      log.warn('No identity available for outbox processing')
      return
    }

    for (const entry of entries) {
      // Check if expired
      if (entry.expiresAt <= now) {
        await db
          .update(haexInviteOutbox)
          .set({ status: OutboxStatus.EXPIRED })
          .where(eq(haexInviteOutbox.id, entry.id))
        // Delete token if expired > 2 weeks (keep for UI display)
        const twoWeeksMs = 14 * 24 * 60 * 60 * 1000
        if (Date.now() - new Date(entry.expiresAt).getTime() > twoWeeksMs) {
          await db.delete(haexInviteTokens).where(eq(haexInviteTokens.id, entry.tokenId))
          log.info(`Deleted stale invite token ${entry.tokenId} (expired > 2 weeks)`)
        }
        log.info(`Outbox entry ${entry.id} expired`)
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
      if (token.expiresAt <= now) {
        await db
          .update(haexInviteOutbox)
          .set({ status: OutboxStatus.EXPIRED })
          .where(eq(haexInviteOutbox.id, entry.id))
        const twoWeeksMs = 14 * 24 * 60 * 60 * 1000
        if (Date.now() - new Date(token.expiresAt).getTime() > twoWeeksMs) {
          await db.delete(haexInviteTokens).where(eq(haexInviteTokens.id, token.id))
          log.info(`Deleted stale invite token ${token.id} (expired > 2 weeks)`)
        }
        continue
      }

      const capabilities: string[] = JSON.parse(token.capabilities)

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
          inviterDid: identity.did,
          inviterLabel: identity.label || null,
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

  return { createOutboxEntryAsync, processOutboxAsync }
}
