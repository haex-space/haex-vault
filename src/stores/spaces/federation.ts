import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { createServerRelayUcanAsync, getUcanForSpaceAsync } from '@/utils/auth/ucanStore'
import { createLogger } from '@/stores/logging'

const log = createLogger('SPACES:FEDERATION')

const normalizeUrl = (url: string) => url.replace(/\/+$/, '').toLowerCase()

/**
 * Check if an invite targets a different server than the user's own.
 * Returns the user's relay server URL if cross-server, null if same-server.
 */
export function detectCrossServerInvite(originServerUrl: string, userServerUrl: string | undefined): string | null {
  if (!userServerUrl) return null
  if (normalizeUrl(originServerUrl) === normalizeUrl(userServerUrl)) return null
  return userServerUrl
}

/** Fetch the relay server's federation DID. */
export async function getRelayServerDid(relayServerUrl: string): Promise<string> {
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
export async function setupFederationForSpace(
  relayServerUrl: string,
  originServerUrl: string,
  spaceId: string,
  identity: { id: string; did: string; privateKey: string },
) {
  // 1. Get relay server's DID and origin server's DID
  const relayServerDid = await getRelayServerDid(relayServerUrl)
  const originServerDid = await getRelayServerDid(originServerUrl)

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
