import { invoke } from '@tauri-apps/api/core'
import { fetchWithUcanAuth, getUcanForSpaceAsync } from '@/utils/auth/ucanStore'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { DidAuthAction } from '@haex-space/ucan'
import { createLogger } from '@/stores/logging'
import { toBase64, fromBase64 } from '~/utils/encoding'

const log = createLogger('MLS_DELIVERY')

export interface MlsMessage {
  id: number
  senderPublicKey: string
  messageType: 'commit' | 'application'
  payload: string // base64
  epoch: number | null
  createdAt: string
}

export interface MlsWelcome {
  id: number
  payload: string // base64
  createdAt: string
}

interface AuthContext {
  privateKey: string
  did: string
}

function requireUcan(spaceId: string): string {
  const ucan = getUcanForSpaceAsync(spaceId)
  if (!ucan) throw new Error(`No UCAN token available for space ${spaceId}`)
  return ucan
}

/**
 * MLS Delivery Service for a specific space on a specific server.
 * Handles KeyPackage, Message, and Welcome transport between client and server.
 *
 * Auth is resolved internally:
 * - UCAN from ucanStore (for space-scoped operations)
 * - DID-Auth via AuthContext (for invite accept/decline)
 */
export function useMlsDelivery(originUrl: string, spaceId: string, auth: AuthContext) {
  const baseUrl = `${originUrl}/spaces/${spaceId}`

  // ===========================================================================
  // Key Packages
  // ===========================================================================

  /**
   * Upload KeyPackages so other members can add us to MLS groups.
   * Generates packages via Rust and uploads to server.
   */
  async function uploadKeyPackagesAsync(count: number = 10): Promise<void> {
    const packages: number[][] = await invoke('mls_get_key_packages', { count })
    const ucan = requireUcan(spaceId)

    const body = JSON.stringify({
      keyPackages: packages.map((p) => toBase64(new Uint8Array(p))),
    })

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/key-packages`,
      spaceId,
      ucan,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to upload key packages: ${error.error || response.statusText}`)
    }

    log.info(`Uploaded ${count} key packages for space ${spaceId}`)
  }

  /**
   * Fetch one unconsumed KeyPackage for a specific DID.
   * Used when finalizing an invite (adding member to MLS group).
   */
  async function fetchKeyPackageAsync(targetDid: string): Promise<{ keyPackage: Uint8Array; includeHistory: boolean }> {
    const ucan = requireUcan(spaceId)

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/key-packages/${encodeURIComponent(targetDid)}`,
      spaceId,
      ucan,
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to fetch key package: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    return {
      keyPackage: fromBase64(data.keyPackage),
      includeHistory: data.includeHistory ?? false,
    }
  }

  // ===========================================================================
  // MLS Messages (Commits + Application Data)
  // ===========================================================================

  /**
   * Send an MLS message (commit or application) to the space.
   */
  async function sendMessageAsync(
    payload: Uint8Array,
    messageType: 'commit' | 'application',
    epoch?: number,
    groupInfo?: Uint8Array,
  ): Promise<number> {
    const ucan = requireUcan(spaceId)

    const body = JSON.stringify({
      payload: toBase64(payload),
      messageType,
      epoch,
      groupInfo: groupInfo ? toBase64(groupInfo) : undefined,
    })

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/messages`,
      spaceId,
      ucan,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to send MLS message: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    return data.messageId
  }

  /**
   * Fetch MLS messages after a given message ID (polling).
   */
  async function fetchMessagesAsync(afterId: number = 0, limit: number = 100): Promise<MlsMessage[]> {
    const ucan = requireUcan(spaceId)

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/messages?after=${afterId}&limit=${limit}`,
      spaceId,
      ucan,
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to fetch MLS messages: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    return data.messages
  }

  // ===========================================================================
  // Welcome Messages
  // ===========================================================================

  /**
   * Send a Welcome message to a specific recipient (after MLS add_member).
   */
  async function sendWelcomeAsync(recipientDid: string, welcome: Uint8Array): Promise<void> {
    const ucan = requireUcan(spaceId)

    const body = JSON.stringify({
      recipientDid,
      payload: toBase64(welcome),
    })

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/welcome`,
      spaceId,
      ucan,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to send welcome: ${error.error || response.statusText}`)
    }

    log.info(`Welcome sent to ${recipientDid} for space ${spaceId}`)
  }

  /**
   * Fetch own unconsumed Welcome messages (marks them as consumed on server).
   */
  async function fetchWelcomesAsync(): Promise<{ id: number; payload: Uint8Array }[]> {
    const ucan = requireUcan(spaceId)

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/welcome`,
      spaceId,
      ucan,
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to fetch welcomes: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    return (data.welcomes as MlsWelcome[]).map((w) => ({ id: w.id, payload: fromBase64(w.payload) }))
  }

  async function ackWelcomeAsync(welcomeId: number): Promise<void> {
    const ucan = requireUcan(spaceId)

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/welcome/${welcomeId}`,
      spaceId,
      ucan,
      { method: 'DELETE' },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      log.warn(`Failed to ACK welcome ${welcomeId}: ${error.error || response.statusText}`)
    }
  }

  // ===========================================================================
  // Invite Accept (DID-Auth, not UCAN — invitee is not yet a member)
  // ===========================================================================

  /**
   * Accept an invite and upload KeyPackages in one request.
   * Uses DID-Auth because the invitee doesn't have a UCAN yet.
   */
  async function acceptInviteAsync(inviteId: string, keyPackageCount: number = 10): Promise<void> {
    const packages: number[][] = await invoke('mls_get_key_packages', { count: keyPackageCount })

    const body = JSON.stringify({
      keyPackages: packages.map((p) => toBase64(new Uint8Array(p))),
    })

    const response = await fetchWithDidAuth(
      `${baseUrl}/invites/${inviteId}/accept`,
      auth.privateKey,
      auth.did,
      DidAuthAction.AcceptInvite,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to accept invite: ${error.error || response.statusText}`)
    }

    log.info(`Invite ${inviteId} accepted, ${keyPackageCount} key packages uploaded`)
  }

  // ===========================================================================
  // Rejoin via External Commit (Epoch-Gap Recovery)
  // ===========================================================================

  /**
   * Request GroupInfo from the server for External Commit rejoin.
   * Server validates UCAN membership before returning GroupInfo.
   */
  async function requestRejoinAsync(): Promise<Uint8Array> {
    const ucan = requireUcan(spaceId)

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/rejoin`,
      spaceId,
      ucan,
      { method: 'POST' },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to request rejoin: ${error.error || response.statusText}`)
    }

    const data = await response.json()
    return fromBase64(data.groupInfo)
  }

  /**
   * Submit an External Commit to rejoin the MLS group.
   * Server validates the DID in the commit has a valid UCAN.
   */
  async function submitExternalCommitAsync(commit: Uint8Array): Promise<void> {
    const ucan = requireUcan(spaceId)

    const body = JSON.stringify({
      commit: toBase64(commit),
    })

    const response = await fetchWithUcanAuth(
      `${baseUrl}/mls/external-commit`,
      spaceId,
      ucan,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      },
    )

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to submit external commit: ${error.error || response.statusText}`)
    }

    log.info(`External commit submitted for space ${spaceId}`)
  }

  /**
   * Full rejoin flow: request GroupInfo → create External Commit → submit.
   * Returns the new epoch key on success.
   */
  async function rejoinAsync(): Promise<{ epoch: number; key: number[] }> {
    log.info(`Starting rejoin for space ${spaceId}`)

    // 1. Get GroupInfo from server
    const groupInfo = await requestRejoinAsync()

    // 2. Create External Commit locally
    const result: { commit: number[]; epochKey: { epoch: number; key: number[] } } =
      await invoke('mls_join_by_external_commit', {
        spaceId,
        groupInfo: Array.from(groupInfo),
      })

    // 3. Submit the commit to server for distribution
    await submitExternalCommitAsync(new Uint8Array(result.commit))

    // 4. Persist the new epoch key
    await invoke('mls_export_epoch_key', { spaceId })

    log.info(`Rejoin completed for space ${spaceId}, new epoch: ${result.epochKey.epoch}`)
    return result.epochKey
  }

  return {
    uploadKeyPackagesAsync,
    fetchKeyPackageAsync,
    sendMessageAsync,
    fetchMessagesAsync,
    sendWelcomeAsync,
    fetchWelcomesAsync,
    ackWelcomeAsync,
    acceptInviteAsync,
    requestRejoinAsync,
    submitExternalCommitAsync,
    rejoinAsync,
  }
}
