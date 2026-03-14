import { base58btcEncode, base58btcDecode } from '@haex-space/vault-sdk'
import type { SpaceInvite } from '@haex-space/vault-sdk'

const INVITE_PREFIX = 'haexvault://invite/'

/**
 * Encodes a SpaceInvite into a haex://invite/<base58> link.
 */
export function encodeInviteLink(invite: SpaceInvite): string {
  const json = JSON.stringify(invite)
  const bytes = new TextEncoder().encode(json)
  return INVITE_PREFIX + base58btcEncode(bytes)
}

/**
 * Decodes a haex://invite/<base58> link back into a SpaceInvite.
 * Also accepts raw base58 without the prefix.
 */
export function decodeInviteLink(link: string): SpaceInvite {
  const encoded = link.startsWith(INVITE_PREFIX)
    ? link.slice(INVITE_PREFIX.length)
    : link

  const bytes = base58btcDecode(encoded)
  const json = new TextDecoder().decode(bytes)
  return JSON.parse(json) as SpaceInvite
}

/**
 * Checks if a string looks like an invite link.
 */
export function isInviteLink(str: string): boolean {
  return str.startsWith(INVITE_PREFIX)
}
