/**
 * Token Manager
 * Manages DID authentication tokens for sync server communication.
 * Replaces the Supabase client which was only used as a token container.
 */

import { shallowRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { engineLog as log } from './types'

/**
 * Callback to resolve backend info needed for DID re-authentication.
 * Returns null if the backend can't be resolved (e.g., no vault open).
 */
export type ReauthContextResolver = () => Promise<{
  serverUrl: string
  did: string
  privateKey: string
} | null>

// Module state
export const isInitializedRef = shallowRef(false)
export const currentBackendIdRef = shallowRef<string | null>(null)
let cachedAccessToken: string | null = null
let cachedRefreshToken: string | null = null
let reauthResolver: ReauthContextResolver | null = null
let lastReauthAttempt = 0
const REAUTH_COOLDOWN_MS = 30_000

/**
 * Central token setter — keeps JS cache and Rust backend in sync.
 */
const updateCachedToken = (token: string | null): void => {
  cachedAccessToken = token
  invoke('set_auth_token', { token }).catch(() => {})
}

/**
 * Initializes the token manager for a specific backend.
 * No HTTP calls needed — just stores the backend ID.
 */
export const initTokenManager = (backendId: string): void => {
  if (isInitializedRef.value && currentBackendIdRef.value === backendId) {
    return
  }

  // Reset state if switching backends
  if (currentBackendIdRef.value !== backendId) {
    updateCachedToken(null)
    cachedRefreshToken = null
  }

  currentBackendIdRef.value = backendId
  isInitializedRef.value = true
}

/**
 * Stores session tokens from DID authentication.
 */
export const setSession = (tokens: { access_token: string; refresh_token: string }): void => {
  updateCachedToken(tokens.access_token)
  cachedRefreshToken = tokens.refresh_token
}

/**
 * Registers a callback that resolves the DID auth context for the current backend.
 */
export const setReauthResolver = (resolver: ReauthContextResolver | null): void => {
  reauthResolver = resolver
}

/**
 * Performs DID-based authentication against the sync server.
 * Returns { access_token, refresh_token } on success.
 */
export const didAuthenticateAsync = async (
  serverUrl: string,
  did: string,
  privateKeyBase64: string,
): Promise<{ access_token: string; refresh_token: string }> => {
  const challengeRes = await fetch(`${serverUrl}/identity-auth/challenge`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ did }),
  })

  if (!challengeRes.ok) {
    const errorData = await challengeRes.json().catch(() => ({ error: 'Unknown error' }))
    throw new Error(`DID challenge failed: ${errorData.error || 'Unknown error'}`)
  }

  const { nonce } = await challengeRes.json()

  const privateKey = await importUserPrivateKeyAsync(privateKeyBase64)
  const sig = await crypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(nonce),
  )
  const signature = btoa(String.fromCharCode(...new Uint8Array(sig)))

  const verifyRes = await fetch(`${serverUrl}/identity-auth/verify`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ did, nonce, signature }),
  })

  if (!verifyRes.ok) {
    const errorData = await verifyRes.json().catch(() => ({ error: 'Unknown error' }))
    throw new Error(`DID verify failed: ${errorData.error || 'Unknown error'}`)
  }

  return verifyRes.json()
}

/**
 * Shared promise so parallel callers wait on the same re-auth attempt.
 */
let pendingReauthPromise: Promise<string | null> | null = null

/**
 * Attempts to re-authenticate via DID challenge when token is expired.
 * Parallel calls share the same promise — no duplicate auth requests.
 */
export const attemptDidReauthAsync = async (): Promise<string | null> => {
  if (!reauthResolver || !isInitializedRef.value) return null

  if (pendingReauthPromise) {
    log.debug('DID re-auth: waiting for ongoing attempt...')
    return pendingReauthPromise
  }

  const now = Date.now()
  if (now - lastReauthAttempt < REAUTH_COOLDOWN_MS) {
    log.warn(`DID re-auth: cooldown active (${Math.round((REAUTH_COOLDOWN_MS - (now - lastReauthAttempt)) / 1000)}s remaining)`)
    return null
  }

  lastReauthAttempt = now
  pendingReauthPromise = (async () => {
    try {
      const ctx = await reauthResolver!()
      if (!ctx) {
        log.warn('DID re-auth: no context available (vault not open?)')
        return null
      }

      log.info('DID re-auth: token expired, re-authenticating via DID challenge...')
      const session = await didAuthenticateAsync(ctx.serverUrl, ctx.did, ctx.privateKey)

      setSession(session)
      lastReauthAttempt = 0
      log.info('DID re-auth: successfully re-authenticated')
      return session.access_token
    } catch (e) {
      log.error('DID re-auth failed:', e)
      return null
    }
  })()

  try {
    return await pendingReauthPromise
  } finally {
    pendingReauthPromise = null
  }
}

/**
 * Checks if a JWT token is expired or about to expire (within 30s buffer)
 */
const isTokenExpired = (token: string): boolean => {
  try {
    const parts = token.split('.')
    if (parts.length < 2 || !parts[1]) return true
    const payload = JSON.parse(atob(parts[1]))
    const expiresAt = payload.exp * 1000
    return Date.now() >= expiresAt - 30_000
  } catch {
    return true
  }
}

/**
 * Gets the current auth token, automatically re-authenticating if expired.
 */
export const getAuthTokenAsync = async (): Promise<string | null> => {
  if (!cachedAccessToken) {
    log.warn('No auth token available — attempting DID re-authentication...')
    return attemptDidReauthAsync()
  }

  if (isTokenExpired(cachedAccessToken)) {
    log.info('Auth token expired, re-authenticating via DID...')
    return attemptDidReauthAsync()
  }

  return cachedAccessToken
}

/**
 * Caches an access token directly.
 */
export const cacheAccessToken = (token: string): void => {
  updateCachedToken(token)
}

/**
 * Resets all token manager state.
 */
export const resetTokenManager = (): void => {
  isInitializedRef.value = false
  currentBackendIdRef.value = null
  cachedRefreshToken = null
  updateCachedToken(null)
}

/**
 * Shared promise so parallel 401s wait on the same re-auth attempt.
 */
let pendingReauth: Promise<string | null> | null = null

/**
 * Fetch wrapper that automatically retries with DID re-auth on 401 responses.
 */
export const fetchWithReauthAsync = async (
  url: string,
  init: RequestInit,
): Promise<Response> => {
  const response = await fetch(url, init)
  if (response.status !== 401) return response

  if (!pendingReauth) {
    log.warn('Server returned 401 — attempting DID re-authentication...')
    pendingReauth = attemptDidReauthAsync().finally(() => {
      pendingReauth = null
    })
  } else {
    log.debug('Server returned 401 — waiting for ongoing re-auth...')
  }

  const newToken = await pendingReauth
  if (!newToken) return response

  const headers = new Headers(init.headers)
  headers.set('Authorization', `Bearer ${newToken}`)
  return fetch(url, { ...init, headers })
}
