import { fetch as tauriFetch } from '@tauri-apps/plugin-http'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import type { SelectHaexMarketplaces } from '@/database/schemas/marketplaces'

/** Identity context needed when auth_type is 'did'. Caller loads it from the identity store. */
export interface DidIdentityContext {
  did: string
  privateKey: string
}

/**
 * Returns a fetch-compatible function that adds the correct auth header for this marketplace row.
 * For auth_type='did', pass the resolved identity context as the second argument.
 */
export function buildAuthedFetch(
  row: SelectHaexMarketplaces,
  didContext?: DidIdentityContext,
): (input: string, init?: RequestInit) => Promise<Response> {
  switch (row.authType) {
    case 'bearer':
      return (input, init) =>
        tauriFetch(input, {
          ...init,
          headers: { ...init?.headers, Authorization: `Bearer ${row.authToken}` },
        }) as unknown as Promise<Response>

    case 'basic': {
      const creds = btoa(`${row.authUsername}:${row.authPassword}`)
      return (input, init) =>
        tauriFetch(input, {
          ...init,
          headers: { ...init?.headers, Authorization: `Basic ${creds}` },
        }) as unknown as Promise<Response>
    }

    case 'did':
      if (!didContext) {
        throw new Error(
          `Marketplace ${row.name}: auth_type=did requires a didContext (load identity ${row.authIdentityId} first)`,
        )
      }
      return (input, init) =>
        fetchWithDidAuth(input, didContext.privateKey, didContext.did, 'marketplace:list', init)

    default: // 'none'
      return (input, init) => tauriFetch(input, init) as unknown as Promise<Response>
  }
}
