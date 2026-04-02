const WEB_INVITE_PREFIX = 'https://haex.space/invite'
const APP_INVITE_PREFIX = 'haexvault://invite/'
const LOCAL_INVITE_PREFIX = 'haexvault://invite/local'

export interface InviteTokenLink {
  serverUrl: string
  spaceId: string
  tokenId: string
}

export interface LocalInviteLink {
  spaceId: string
  tokenId: string
  spaceEndpoints: string[]
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
 * New format: haexvault://invite/local?data=BASE64_JSON
 * Legacy format: haexvault://invite/local?endpoint=ID&space=ID&token=TOKEN_ID
 */
export function parseLocalInviteLink(link: string): LocalInviteLink | null {
  try {
    const url = new URL(link)

    // New format: Base64-encoded JSON payload
    const data = url.searchParams.get('data')
    if (data) {
      return JSON.parse(atob(decodeURIComponent(data)))
    }

    // Legacy format: individual params (backwards compat)
    const endpointId = url.searchParams.get('endpoint')
    const spaceId = url.searchParams.get('space')
    const tokenId = url.searchParams.get('token')
    if (!endpointId || !spaceId || !tokenId) return null
    return { spaceId, tokenId, spaceEndpoints: [endpointId] }
  } catch {
    return null
  }
}

/**
 * Build a local invite link with Base64-encoded JSON payload.
 */
export function buildLocalInviteLink(link: LocalInviteLink): string {
  const payload = btoa(JSON.stringify(link))
  return `${LOCAL_INVITE_PREFIX}?data=${encodeURIComponent(payload)}`
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
