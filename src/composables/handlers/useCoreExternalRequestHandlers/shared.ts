import { invoke } from '@tauri-apps/api/core'
import { parse as parseTld } from 'tldts'
import type { ExternalCoreResponse } from './types'

export const respondAsync = async (response: ExternalCoreResponse): Promise<void> => {
  await invoke('external_bridge_respond', {
    requestId: response.requestId,
    success: response.success,
    data: response.data ?? null,
    error: response.error ?? null,
  })
}

export const errorResponse = (requestId: string, message: string): ExternalCoreResponse => ({
  requestId,
  success: false,
  error: message,
})

export const toErrorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : 'Unknown error'

/**
 * Reduce a URL (or bare host) into `{ hostname, registrableDomain }`.
 *
 * Used by the get-items URL matcher so an entry stored as `example.de`
 * matches when the browser is on `www.example.de` or `app.example.de`.
 * `registrableDomain` is the eTLD+1 from the Public Suffix List
 * (`example.de`, `example.co.uk`, …) and is `null` for IPs, `localhost`,
 * or intranet names — callers then fall back to hostname equality.
 *
 * `allowPrivateDomains: true` keeps multi-tenant private suffixes
 * (`*.github.io`, `*.herokuapp.com`, …) distinct so credentials for one
 * tenant don't cross-match another sharing the same private suffix.
 */
export const describeUrlForMatching = (
  input: string,
): { hostname: string | null; registrableDomain: string | null } => {
  const tryConstruct = (raw: string): URL | null => {
    try {
      return new URL(raw)
    } catch {
      return null
    }
  }
  // Stored entries are often just "example.de" without a scheme — URL needs one.
  const parsed = tryConstruct(input) ?? tryConstruct(`https://${input}`)
  if (!parsed) return { hostname: null, registrableDomain: null }
  const hostname = parsed.hostname.toLowerCase()
  if (!hostname) return { hostname: null, registrableDomain: null }
  const { domain } = parseTld(hostname, { allowPrivateDomains: true })
  return { hostname, registrableDomain: domain ?? null }
}
