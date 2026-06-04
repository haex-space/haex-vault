/**
 * Token Manager
 * Manages DID authentication tokens for sync server communication.
 * Supports multiple backends concurrently via Map-based state.
 */

import { computed, shallowRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { fetch } from '@tauri-apps/plugin-http'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { engineLog as log } from './types'

/**
 * Callback to resolve backend info needed for DID re-authentication.
 * Returns null if the backend can't be resolved (e.g., no vault open).
 */
export type ReauthContextResolver = () => Promise<{
  homeServerUrl: string
  did: string
  privateKey: string
} | null>

interface TokenState {
  accessToken: string | null
  refreshToken: string | null
  reauthResolver: ReauthContextResolver | null
  lastReauthAttempt: number
  inflightReauth: Promise<string | null> | null
}

const tokenStates = new Map<string, TokenState>()
export const currentBackendIdRef = shallowRef<string | null>(null)

/**
 * Whether any backend has been initialized.
 */
export const isInitializedRef = computed(() => tokenStates.size > 0)

const REAUTH_COOLDOWN_MS = 30_000

const getOrCreateState = (backendId: string): TokenState => {
  let state = tokenStates.get(backendId)
  if (!state) {
    state = {
      accessToken: null,
      refreshToken: null,
      reauthResolver: null,
      lastReauthAttempt: 0,
      inflightReauth: null,
    }
    tokenStates.set(backendId, state)
  }
  return state
}

const resolveBackendId = (backendId?: string): string => {
  const resolved = backendId ?? currentBackendIdRef.value
  if (!resolved) {
    throw new Error('No backendId provided and no current backend set')
  }
  return resolved
}

/**
 * Central token setter — keeps JS cache and Rust backend in sync.
 * Only invokes Rust set_auth_token when the token belongs to the current backend.
 */
const updateCachedToken = (backendId: string, token: string | null): void => {
  const state = getOrCreateState(backendId)
  state.accessToken = token
  if (backendId === currentBackendIdRef.value) {
    invoke('set_auth_token', { token }).catch((e) => log.debug('Failed to set auth token:', e))
  }
}

/**
 * Initializes the token manager for a specific backend.
 * Does NOT clear other backends' tokens when switching.
 */
export const initTokenManager = (backendId: string): void => {
  getOrCreateState(backendId)
  currentBackendIdRef.value = backendId
}

/**
 * Stores session tokens from DID authentication.
 */
export const setSession = (backendId: string, tokens: { access_token: string; refresh_token: string }): void => {
  const state = getOrCreateState(backendId)
  updateCachedToken(backendId, tokens.access_token)
  state.refreshToken = tokens.refresh_token
}

/**
 * Registers a callback that resolves the DID auth context for a specific backend.
 */
export const setReauthResolver = (backendId: string, resolver: ReauthContextResolver | null): void => {
  const state = getOrCreateState(backendId)
  state.reauthResolver = resolver
}

/**
 * Performs DID-based authentication against the sync server.
 * Returns { access_token, refresh_token } on success.
 */
export const didAuthenticateAsync = async (
  homeServerUrl: string,
  did: string,
  privateKeyBase64: string,
): Promise<{ access_token: string; refresh_token: string }> => {
  const challengeRes = await fetch(`${homeServerUrl}/identity-auth/challenge`, {
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

  const verifyRes = await fetch(`${homeServerUrl}/identity-auth/verify`, {
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
 * Attempts to re-authenticate via DID challenge when token is expired.
 * Parallel callers share `state.inflightReauth` — exactly one challenge
 * round trip per re-auth window.
 */
export const attemptDidReauthAsync = async (backendId?: string): Promise<string | null> => {
  const id = resolveBackendId(backendId)
  const state = getOrCreateState(id)

  if (!state.reauthResolver) return null

  if (state.inflightReauth) {
    log.debug('DID re-auth: waiting for ongoing attempt...')
    return state.inflightReauth
  }

  const now = Date.now()
  if (now - state.lastReauthAttempt < REAUTH_COOLDOWN_MS) {
    log.warn(`DID re-auth: cooldown active (${Math.round((REAUTH_COOLDOWN_MS - (now - state.lastReauthAttempt)) / 1000)}s remaining)`)
    return null
  }

  state.lastReauthAttempt = now
  const inflight = (async () => {
    try {
      const ctx = await state.reauthResolver!()
      if (!ctx) {
        log.warn('DID re-auth: no context available (vault not open?)')
        return null
      }

      log.info('DID re-auth: token expired, re-authenticating via DID challenge...')
      const session = await didAuthenticateAsync(ctx.homeServerUrl, ctx.did, ctx.privateKey)

      setSession(id, session)
      state.lastReauthAttempt = 0
      log.info('DID re-auth: successfully re-authenticated')
      return session.access_token
    } catch (e) {
      log.error('DID re-auth failed:', e)
      return null
    }
  })()
  state.inflightReauth = inflight

  try {
    return await inflight
  } finally {
    if (state.inflightReauth === inflight) state.inflightReauth = null
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
export const getAuthTokenAsync = async (backendId?: string): Promise<string | null> => {
  const id = resolveBackendId(backendId)
  const state = getOrCreateState(id)

  if (!state.accessToken) {
    log.warn('No auth token available — attempting DID re-authentication...')
    return attemptDidReauthAsync(id)
  }

  if (isTokenExpired(state.accessToken)) {
    log.info('Auth token expired, re-authenticating via DID...')
    return attemptDidReauthAsync(id)
  }

  return state.accessToken
}

/**
 * Caches an access token directly.
 */
export const cacheAccessToken = (backendId: string, token: string): void => {
  updateCachedToken(backendId, token)
}

/**
 * Checks if a specific backend has been initialized.
 */
export const isTokenManagerInitializedForBackend = (backendId: string): boolean => {
  return tokenStates.has(backendId)
}

/**
 * Clears token state for a specific backend.
 */
export const clearTokenState = (backendId: string): void => {
  tokenStates.delete(backendId)
}

/**
 * Clears all token states.
 */
export const clearAllTokenStates = (): void => {
  tokenStates.clear()
  currentBackendIdRef.value = null
}

/**
 * Resets all token manager state.
 */
export const resetTokenManager = (): void => {
  // Inform Rust that there is no auth token
  invoke('set_auth_token', { token: null }).catch((e) => log.debug('Failed to reset auth token:', e))
  tokenStates.clear()
  currentBackendIdRef.value = null
}

/**
 * Fetch wrapper that automatically retries with DID re-auth on 401 responses.
 */
export const fetchWithReauthAsync = async (
  url: string,
  init: RequestInit,
  backendId?: string,
): Promise<Response> => {
  const id = resolveBackendId(backendId)
  const response = await fetch(url, init)
  if (response.status !== 401) return response

  log.warn('Server returned 401 — attempting DID re-authentication...')
  const newToken = await attemptDidReauthAsync(id)
  if (!newToken) return response

  const headers = new Headers(init.headers)
  headers.set('Authorization', `Bearer ${newToken}`)
  return fetch(url, { ...init, headers })
}
