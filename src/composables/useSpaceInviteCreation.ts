import { invoke } from '@tauri-apps/api/core'
import type { SelectHaexIdentities } from '~/database/schemas'
import { buildLocalInviteLink } from '~/utils/inviteLink'
import { createLogger } from '@/stores/logging'

const log = createLogger('SPACES:INVITE-CREATE')

export interface InviteContactsPayload {
  spaceId: string
  serverUrl: string
  identityId: string
  contacts: SelectHaexIdentities[]
  capabilities: string[]
  includeHistory: boolean
  expiresInSeconds: number
  /** When true, server-side invites are skipped (purely P2P/QUIC). */
  localOnly?: boolean
}

export interface CreateLocalLinkPayload {
  spaceId: string
  capability: string
  maxUses: number
  expiresInSeconds: number
  includeHistory: boolean
  spaceEndpoints: string[]
}

export interface CreateOnlineLinkPayload {
  spaceId: string
  serverUrl: string
  capability: string
  maxUses: number
  expiresInSeconds: number
  label?: string
}

export interface InviteLinkResult {
  link: string
  expiresAt: string
}

/**
 * Encapsulates the three branches of invite creation (contact / local link /
 * online link). Extracted out of `SpaceInviteDialog` so the dialog can stay
 * infrastructure-free and the logic is testable in isolation.
 *
 * All functions throw on failure — callers decide how to surface errors.
 */
export function useSpaceInviteCreation() {
  const spacesStore = useSpacesStore()
  const identityStore = useIdentityStore()

  /**
   * Sends a dual-channel invite (server + QUIC) to each selected contact.
   * - Server invite is best-effort: failure logs a warning but continues.
   * - QUIC invite is attempted when the contact has known endpoints; its
   *   failure is fatal only when the server invite also failed.
   */
  const inviteContactsAsync = async (
    payload: InviteContactsPayload,
  ): Promise<void> => {
    log.info(
      `Inviting ${payload.contacts.length} contact(s) to space ${payload.spaceId}`,
    )

    for (const contact of payload.contacts) {
      const inviteeDid = contact.did
      const claims = await identityStore.getClaimsAsync(contact.id)
      const endpointIds = claims
        .filter(
          (c) => c.type === 'endpointId' || c.type.startsWith('device:'),
        )
        .map((c) => c.value)

      log.info(
        `Processing contact "${contact.name}" (did: ${inviteeDid.slice(0, 24)}..., ${endpointIds.length} endpoint(s))`,
      )

      let serverInviteId: string | undefined

      // 1. Server invite (skip for local-only spaces)
      if (!payload.localOnly && payload.serverUrl) {
        try {
          const result = await spacesStore.inviteMemberAsync(
            payload.serverUrl,
            payload.spaceId,
            inviteeDid,
            payload.capabilities[0]!,
            payload.identityId,
            payload.includeHistory,
          )
          serverInviteId = result.inviteId
          log.info(`Server invite created: ${result.inviteId}`)
        } catch (error) {
          log.warn(
            `Server invite failed for "${contact.name}", continuing with QUIC`,
            error,
          )
        }
      }

      // 2. Always queue QUIC PushInvite (DB-based, works for both local and online spaces)
      if (endpointIds.length > 0) {
        try {
          await spacesStore.queueQuicInviteAsync({
            spaceId: payload.spaceId,
            tokenId: serverInviteId,
            contactDid: inviteeDid,
            contactEndpointIds: endpointIds,
            capabilities: payload.capabilities,
            includeHistory: payload.includeHistory,
            expiresInSeconds: payload.expiresInSeconds,
          })
          log.info(
            `QUIC invite queued for "${contact.name}" → ${endpointIds.length} endpoint(s)`,
          )
        } catch (error) {
          // If server invite succeeded, QUIC failure is not fatal
          if (!serverInviteId) throw error
          log.warn(
            `QUIC invite failed for "${contact.name}", server invite was sent`,
            error,
          )
        }
      } else {
        log.warn(
          `No endpoints for "${contact.name}", QUIC invite skipped`,
        )
      }
    }

    log.info(`All contact invites processed for space ${payload.spaceId}`)
  }

  /**
   * Creates a local (QUIC) invite link — no server involvement.
   * Returns the shareable link and a pre-computed expiry timestamp.
   */
  const createLocalInviteLinkAsync = async (
    payload: CreateLocalLinkPayload,
  ): Promise<InviteLinkResult> => {
    log.info(
      `Creating local invite link for space ${payload.spaceId} (maxUses: ${payload.maxUses})`,
    )

    const tokenId = await invoke<string>('local_delivery_create_invite', {
      spaceId: payload.spaceId,
      targetDid: null,
      capability: payload.capability,
      maxUses: payload.maxUses,
      expiresInSeconds: payload.expiresInSeconds,
      includeHistory: payload.includeHistory,
    })

    const link = buildLocalInviteLink({
      spaceId: payload.spaceId,
      tokenId,
      spaceEndpoints: payload.spaceEndpoints,
    })

    const expiresAt = new Date(
      Date.now() + payload.expiresInSeconds * 1000,
    ).toISOString()

    log.info(
      `Local invite link created (token: ${tokenId}, endpoints: ${payload.spaceEndpoints.length})`,
    )

    return { link, expiresAt }
  }

  /**
   * Creates an online invite token on the given server and builds the
   * shareable link. Expiry timestamp comes from the server response.
   */
  const createOnlineInviteLinkAsync = async (
    payload: CreateOnlineLinkPayload,
  ): Promise<InviteLinkResult> => {
    log.info(
      `Creating online invite token for space ${payload.spaceId} (maxUses: ${payload.maxUses})`,
    )

    const result = await spacesStore.createInviteTokenAsync(
      payload.serverUrl,
      payload.spaceId,
      {
        capability: payload.capability,
        maxUses: payload.maxUses,
        expiresInSeconds: payload.expiresInSeconds,
        label: payload.label,
      },
    )

    const link = spacesStore.buildInviteLink(
      payload.serverUrl,
      payload.spaceId,
      result.tokenId,
    )

    log.info(
      `Online invite link created (token: ${result.tokenId}, expires: ${result.expiresAt})`,
    )

    return { link, expiresAt: result.expiresAt }
  }

  return {
    inviteContactsAsync,
    createLocalInviteLinkAsync,
    createOnlineInviteLinkAsync,
  }
}
