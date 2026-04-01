const WEB_INVITE_PREFIX = 'https://haex.space/invite'
const APP_INVITE_PREFIX = 'haexvault://invite/'
const LOCAL_INVITE_PREFIX = 'haexvault://invite/local'

export interface InviteTokenLink {
  serverUrl: string
  spaceId: string
  tokenId: string
}

export interface LocalInviteLink {
  endpointId: string
  spaceId: string
  tokenId: string
  relayUrl?: string
}

/**
 * Checks if a string looks like an invite link (web URL, app deep link, or local invite).
 */
export function isInviteLink(str: string): boolean {
  return str.startsWith(WEB_INVITE_PREFIX) || str.startsWith(APP_INVITE_PREFIX) || str.startsWith(LOCAL_INVITE_PREFIX)
}

/**
 * Checks if a string is a local invite link.
 */
export function isLocalInviteLink(str: string): boolean {
  return str.startsWith(LOCAL_INVITE_PREFIX)
}

/**
 * Parse a local invite link.
 * Supports: haexvault://invite/local?endpoint=ID&space=ID&token=TOKEN_ID&relay=URL
 */
export function parseLocalInviteLink(link: string): LocalInviteLink | null {
  try {
    const url = new URL(link)
    const endpointId = url.searchParams.get('endpoint')
    const spaceId = url.searchParams.get('space')
    const tokenId = url.searchParams.get('token')
    const relayUrl = url.searchParams.get('relay') || undefined

    if (!endpointId || !spaceId || !tokenId) return null
    return { endpointId, spaceId, tokenId, relayUrl }
  } catch {
    return null
  }
}

/**
 * Build a local invite link from components.
 */
export function buildLocalInviteLink(
  endpointId: string,
  spaceId: string,
  tokenId: string,
  relayUrl?: string,
): string {
  const params = new URLSearchParams({ endpoint: endpointId, space: spaceId, token: tokenId })
  if (relayUrl) params.set('relay', relayUrl)
  return `${LOCAL_INVITE_PREFIX}?${params.toString()}`
}

/**
 * Parse an invite token link.
 * Supports:
 * - https://haex.space/invite?server=URL&space=ID&token=TOKEN_ID
 * - haexvault://invite/?server=URL&space=ID&token=TOKEN_ID
 */
export function parseInviteTokenLink(link: string): InviteTokenLink | null {
  try {
    const url = new URL(link)
    const serverUrl = url.searchParams.get('server')
    const spaceId = url.searchParams.get('space')
    const tokenId = url.searchParams.get('token')

    if (!serverUrl || !spaceId || !tokenId) return null
    return { serverUrl, spaceId, tokenId }
  } catch {
    return null
  }
}
