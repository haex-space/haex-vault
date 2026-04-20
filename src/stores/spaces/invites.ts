import { eq } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import type { Capability } from '@haex-space/ucan'
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import { haexSpaces, haexInviteTokens } from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { delegateUcanAsync, fetchWithUcanAuth, getUcanForSpaceAsync } from '@/utils/auth/ucanStore'
import { throwIfNotOk } from '@/utils/fetch'
import { SpaceType, SpaceStatus } from '~/database/constants'
import type { SpaceType as SpaceTypeValue } from '~/database/constants'
import { createLogger } from '@/stores/logging'
import { detectCrossServerInvite, setupFederationForSpace } from './federation'
import { addSelfAsSpaceMember } from './members'
import type { SpaceWithType, ResolvedIdentity } from './index'

type DB = SqliteRemoteDatabase<typeof schema>

const log = createLogger('SPACES:INVITES')

/** Fetch with UCAN authorization for space-scoped operations */
function fetchWithSpaceUcanAuth(url: string, spaceId: string, options?: RequestInit) {
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

/**
 * Create a pending invite for a DID to join a space.
 */
export async function inviteMember(
  spaces: SpaceWithType[],
  originUrl: string,
  spaceId: string,
  inviteeDid: string,
  capability: string,
  identity: ResolvedIdentity,
  includeHistory: boolean = false,
): Promise<{ inviteId: string }> {
  const spaceEntry = spaces.find(s => s.id === spaceId)
  if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot invite members to vault space')

  const parentUcan = getUcanForSpaceAsync(spaceId)
  if (!parentUcan) throw new Error('No UCAN available for this space')

  const delegatedUcan = await delegateUcanAsync(
    identity.did,
    identity.privateKey,
    inviteeDid,
    spaceId,
    capability as Capability,
    parentUcan,
  )

  const response = await fetchWithSpaceUcanAuth(
    `${originUrl}/spaces/${spaceId}/invites`,
    spaceId,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ inviteeDid, ucan: delegatedUcan, includeHistory }),
    },
  )

  await throwIfNotOk(response, 'invite member')

  const data = await response.json()
  log.info(`Invited ${inviteeDid} to space ${spaceId} with ${capability}`)
  return { inviteId: data.invite.id }
}

/**
 * Create an invite token (for link or QR code sharing).
 */
export async function createInviteToken(
  spaces: SpaceWithType[],
  originUrl: string,
  spaceId: string,
  options: {
    capability?: string
    maxUses?: number
    expiresInSeconds: number
    label?: string
  },
): Promise<{ tokenId: string; expiresAt: string }> {
  const spaceEntry = spaces.find(s => s.id === spaceId)
  if (spaceEntry?.type === SpaceType.VAULT) throw new Error('Cannot create invite tokens for vault space')

  const response = await fetchWithSpaceUcanAuth(
    `${originUrl}/spaces/${spaceId}/invite-tokens`,
    spaceId,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(options),
    },
  )

  await throwIfNotOk(response, 'create invite token')

  const data = await response.json()
  log.info(`Created invite token for space ${spaceId} (maxUses: ${options.maxUses ?? 1})`)
  return { tokenId: data.token.id, expiresAt: data.token.expiresAt }
}

export function buildInviteLink(originUrl: string, spaceId: string, tokenId: string): string {
  const params = new URLSearchParams({ server: originUrl, space: spaceId, token: tokenId })
  return `https://haex.space/invite?${params.toString()}`
}

/**
 * Claim an invite token (invitee side). Registers DID + uploads KeyPackages.
 * Handles both same-server and cross-server invites transparently.
 */
export async function claimInviteToken(
  db: DB,
  originUrl: string,
  spaceId: string,
  tokenId: string,
  identity: ResolvedIdentity,
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
): Promise<{ capability: string }> {
  const backendsStore = useSyncBackendsStore()
  const userServerUrl = backendsStore.backends[0]?.homeServerUrl
  const relayServerUrl = detectCrossServerInvite(originUrl, userServerUrl)

  // Generate MLS KeyPackages
  const packages: number[][] = await invoke('mls_get_key_packages', { count: 10 })
  const keyPackagesBase64 = packages.map((p: number[]) => btoa(String.fromCharCode(...new Uint8Array(p))))

  const claimBody = JSON.stringify({
    keyPackages: keyPackagesBase64,
    label: identity.name,
  })
  const response = await fetchWithDidAuth(
    `${originUrl}/spaces/${spaceId}/invite-tokens/${tokenId}/claim`,
    identity.privateKey,
    identity.did,
    'accept-invite',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: claimBody,
    },
  )

  await throwIfNotOk(response, 'claim invite')

  const data = await response.json()

  if (relayServerUrl) {
    await setupFederationForSpace(relayServerUrl, originUrl, spaceId, identity)

    await persistSpaceAsync({
      id: spaceId,
      name: '',
      type: SpaceType.ONLINE,
      status: SpaceStatus.ACTIVE,
      ownerIdentityId: identity.id,
      originUrl: relayServerUrl,
      createdAt: new Date().toISOString(),
      capabilities: [],
    })

    log.info(`Claimed cross-server invite for space ${spaceId} (capability: ${data.capability}, relay: ${relayServerUrl})`)
  } else {
    await persistSpaceAsync({
      id: spaceId,
      name: '',
      type: SpaceType.ONLINE,
      status: SpaceStatus.ACTIVE,
      ownerIdentityId: identity.id,
      originUrl: originUrl,
      createdAt: new Date().toISOString(),
      capabilities: [],
    })

    log.info(`Claimed invite token for space ${spaceId} (capability: ${data.capability})`)
  }

  // Add self as space member (non-fatal)
  const identityStore = useIdentityStore()
  await identityStore.loadIdentitiesAsync()
  const myIdentity = identityStore.ownIdentities[0]
  if (myIdentity) {
    const role = data.capability?.replace('space/', '') || 'read'
    await addSelfAsSpaceMember(db, spaceId, myIdentity, role)
  }

  return { capability: data.capability }
}

/**
 * Admin-side: finalize an accepted invite by adding the member to the MLS group.
 */
export async function finalizeInvite(
  originUrl: string,
  spaceId: string,
  inviteeDid: string,
  identity: ResolvedIdentity,
  inviteId?: string,
  capability?: string,
) {
  // 1. If no UCAN on the invite yet (token-based invite), create one now
  if (inviteId && capability) {
    const parentUcan = getUcanForSpaceAsync(spaceId)
    if (parentUcan) {
      const delegatedUcan = await delegateUcanAsync(
        identity.did,
        identity.privateKey,
        inviteeDid,
        spaceId,
        capability as Capability,
        parentUcan,
      )
      await fetchWithSpaceUcanAuth(
        `${originUrl}/spaces/${spaceId}/invites/${inviteId}/ucan`,
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
  const { useMlsDelivery } = await import('@/composables/useMlsDelivery')
  const delivery = useMlsDelivery(originUrl, spaceId, { privateKey: identity.privateKey, did: identity.did })
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
 * Crash-safe: stages each Welcome locally before processing, ACKs on server after success.
 */
export async function processWelcomes(db: DB, originUrl: string, spaceId: string, identity: ResolvedIdentity) {
  const { useMlsDelivery } = await import('@/composables/useMlsDelivery')
  const delivery = useMlsDelivery(originUrl, spaceId, { privateKey: identity.privateKey, did: identity.did })
  const { haexMlsPendingWelcomesNoSync } = await import('~/database/schemas/mls')

  const welcomes = await delivery.fetchWelcomesAsync()
  for (const welcome of welcomes) {
    const stagingId = crypto.randomUUID()

    // Stage locally before processing
    await db.insert(haexMlsPendingWelcomesNoSync).values({
      id: stagingId,
      spaceId,
      welcomePayload: Buffer.from(welcome.payload).toString('base64'),
      source: 'server',
      sourceId: String(welcome.id),
      createdAt: new Date().toISOString(),
    })

    await invoke('mls_process_message', { spaceId, message: Array.from(welcome.payload) })

    // ACK on server + remove staging row
    await delivery.ackWelcomeAsync(welcome.id)
    await db.delete(haexMlsPendingWelcomesNoSync).where(eq(haexMlsPendingWelcomesNoSync.id, stagingId))
  }

  if (welcomes.length > 0) {
    log.info(`Processed ${welcomes.length} MLS welcome(s) for space ${spaceId}`)
  }
}

/**
 * Retry any pending Welcomes that weren't fully processed (crash recovery).
 * Called on app startup.
 */
export async function retryPendingWelcomes(db: DB) {
  const { haexMlsPendingWelcomesNoSync } = await import('~/database/schemas/mls')

  const pending = await db.select().from(haexMlsPendingWelcomesNoSync)
  if (pending.length === 0) return

  log.info(`Retrying ${pending.length} pending Welcome(s) from previous session`)

  for (const row of pending) {
    try {
      const binary = atob(row.welcomePayload)
      const welcomeBytes = new Uint8Array(binary.length)
      for (let i = 0; i < binary.length; i++) welcomeBytes[i] = binary.charCodeAt(i)
      await invoke('mls_process_message', { spaceId: row.spaceId, message: Array.from(welcomeBytes) })
      await db.delete(haexMlsPendingWelcomesNoSync).where(eq(haexMlsPendingWelcomesNoSync.id, row.id))
      log.info(`Recovered Welcome for space ${row.spaceId} (source: ${row.source})`)
    } catch (error) {
      log.warn(`Failed to retry Welcome ${row.id}: ${error}`)
    }
  }
}

/**
 * Accept a local P2P invite. Tries all space endpoints until ClaimInvite succeeds.
 */
export async function acceptLocalInvite(
  db: DB,
  invite: {
    id: string
    spaceId: string
    spaceName?: string | null
    spaceType?: string | null
    originUrl?: string | null
    inviterDid?: string | null
    inviterLabel?: string | null
    inviterAvatar?: string | null
    inviterAvatarOptions?: string | null
    spaceEndpoints: string | null
    tokenId: string | null
  },
  persistSpaceAsync: (space: SpaceWithType) => Promise<void>,
  loadSpacesFromDbAsync: () => Promise<void>,
) {
  if (!invite.spaceEndpoints || !invite.tokenId) {
    throw new Error('Missing invite data for local claim')
  }

  const identityStore = useIdentityStore()
  await identityStore.loadIdentitiesAsync()
  const identity = identityStore.ownIdentities[0]
  if (!identity) throw new Error('No identity available')
  if (!invite.inviterDid) throw new Error('Missing inviter DID for local invite')

  log.info(`acceptLocalInvite: BEFORE ensureIdentityForDidAsync inviterDid=${invite.inviterDid.slice(0, 20)}`)
  const ownerIdentity = await identityStore.ensureIdentityForDidAsync(invite.inviterDid, {
    name: invite.inviterLabel,
    avatar: invite.inviterAvatar,
    avatarOptions: invite.inviterAvatarOptions,
    source: 'space',
  })
  log.info(`acceptLocalInvite: AFTER ensureIdentityForDidAsync ownerIdentity.id=${ownerIdentity.id}`)
  const identityPublicKey = await didKeyToPublicKeyAsync(identity.did)

  const endpoints: string[] = JSON.parse(invite.spaceEndpoints)
  if (endpoints.length === 0) throw new Error('No space endpoints in invite')

  log.info(`ClaimInvite: trying ${endpoints.length} endpoint(s) for space ${invite.spaceId}, token=${invite.tokenId}`)

  let lastError: Error | null = null
  let acceptedEndpoint: string | null = null
  for (const endpointId of endpoints) {
    try {
      log.info(`ClaimInvite: connecting to ${endpointId.slice(0, 16)}...`)
      log.info(`ClaimInvite: BEFORE invoke local_delivery_claim_invite`)
      await invoke('local_delivery_claim_invite', {
        leaderEndpointId: endpointId,
        leaderRelayUrl: null,
        spaceId: invite.spaceId,
        spaceName: invite.spaceName || invite.spaceId.slice(0, 8),
        tokenId: invite.tokenId,
        identityDid: identity.did,
        label: identity.name || null,
        identityPublicKey,
      })
      log.info(`ClaimInvite: AFTER invoke local_delivery_claim_invite — returned OK`)
      log.info(`ClaimInvite: success to ${endpointId.slice(0, 16)}`)
      lastError = null
      acceptedEndpoint = endpointId
      break
    } catch (error) {
      lastError = error instanceof Error ? error : new Error(String(error))
      log.error(`ClaimInvite to ${endpointId.slice(0, 16)} failed: ${lastError.message}`)
    }
  }
  if (lastError) throw lastError

  // Start the peer-side CRDT sync loop so we pull the space's historical
  // state (other members, shares, devices) from the leader. Without this,
  // the invitee sees only locally-inserted rows (their own membership).
  // Non-fatal on failure — Layer 2 (app-startup election) will retry.
  if (acceptedEndpoint) {
    try {
      await invoke('local_delivery_connect', {
        spaceId: invite.spaceId,
        leaderEndpointId: acceptedEndpoint,
        leaderRelayUrl: null,
        identityDid: identity.did,
      })
      log.info(`ClaimInvite: started sync loop to ${acceptedEndpoint.slice(0, 16)}`)
    } catch (error) {
      log.warn(`ClaimInvite: failed to start sync loop: ${error}`)
    }
  }

  // Create or activate the space entry
  const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, invite.spaceId)).limit(1)
  if (existing.length > 0) {
    await db.update(haexSpaces).set({ status: SpaceStatus.ACTIVE }).where(eq(haexSpaces.id, invite.spaceId))
  } else {
    await persistSpaceAsync({
      id: invite.spaceId,
      name: invite.spaceName || invite.spaceId.slice(0, 8),
      type: (invite.spaceType as SpaceTypeValue) || SpaceType.LOCAL,
      status: SpaceStatus.ACTIVE,
      ownerIdentityId: ownerIdentity.id,
      originUrl: invite.originUrl || '',
      createdAt: new Date().toISOString(),
      capabilities: [],
    })
  }

  await loadSpacesFromDbAsync()

  // Add self as space member (non-fatal)
  await addSelfAsSpaceMember(db, invite.spaceId, identity, 'read')

  log.info(`Accepted local invite for space ${invite.spaceId}`)
}

/**
 * Queue a QUIC PushInvite without requiring leader mode.
 */
export async function queueQuicInvite(db: DB, params: {
  spaceId: string
  tokenId?: string
  contactDid: string
  contactEndpointIds: string[]
  capabilities: string[]
  includeHistory: boolean
  expiresInSeconds: number
}) {
  if (params.contactEndpointIds.length === 0) {
    throw new Error('Contact has no known EndpointId — share identities via QR code first')
  }

  const id = params.tokenId || crypto.randomUUID()
  const expiresAt = new Date(Date.now() + params.expiresInSeconds * 1000).toISOString()
  const now = new Date().toISOString()

  await db.insert(haexInviteTokens).values({
    id,
    spaceId: params.spaceId,
    targetDid: params.contactDid,
    capabilities: JSON.stringify(params.capabilities),
    includeHistory: params.includeHistory,
    maxUses: 1,
    currentUses: 0,
    expiresAt,
    createdAt: now,
  })

  const { useInviteOutbox } = await import('@/composables/useInviteOutbox')
  const { createOutboxEntryAsync, processOutboxAsync } = useInviteOutbox()

  for (const endpointId of params.contactEndpointIds) {
    await createOutboxEntryAsync({
      spaceId: params.spaceId,
      tokenId: id,
      targetDid: params.contactDid,
      targetEndpointId: endpointId,
      expiresAt,
    })
  }

  log.info(`Queued QUIC invite for ${params.contactDid} in space ${params.spaceId} (${params.contactEndpointIds.length} endpoint(s))`)

  processOutboxAsync().catch(err =>
    log.warn(`Immediate outbox processing failed (will retry): ${err}`),
  )
}
