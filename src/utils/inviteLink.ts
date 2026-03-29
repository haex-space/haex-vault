const WEB_INVITE_PREFIX = 'https://haex.space/invite'
const APP_INVITE_PREFIX = 'haexvault://invite/'

export interface InviteTokenLink {
  serverUrl: string
  spaceId: string
  tokenId: string
}

/**
 * Checks if a string looks like an invite link (web URL or app deep link).
 */
export function isInviteLink(str: string): boolean {
  return str.startsWith(WEB_INVITE_PREFIX) || str.startsWith(APP_INVITE_PREFIX)
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
