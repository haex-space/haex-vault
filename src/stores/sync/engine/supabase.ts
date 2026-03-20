/**
 * Supabase Client Management
 * Handles Supabase client initialization and authentication
 */

import { shallowRef } from 'vue'
import { createClient } from '@supabase/supabase-js'
import { invoke } from '@tauri-apps/api/core'
import { importUserPrivateKeyAsync } from '@haex-space/vault-sdk'
import { engineLog as log } from './types'

// Use the actual return type of createClient for consistency across the codebase
export type AppSupabaseClient = ReturnType<typeof createClient>

/**
 * Callback to resolve backend info needed for DID re-authentication.
 * Returns null if the backend can't be resolved (e.g., no vault open).
 */
export type ReauthContextResolver = () => Promise<{
  serverUrl: string
  did: string
  privateKey: string
} | null>

// Module state — using shallowRef so Vue computed() can track changes
export const supabaseClientRef = shallowRef<AppSupabaseClient | null>(null)
export const currentBackendIdRef = shallowRef<string | null>(null)
let cachedAccessToken: string | null = null
let reauthResolver: ReauthContextResolver | null = null
let reauthInProgress = false

/**
 * Gets the current Supabase client
 */
export const getSupabaseClient = (): AppSupabaseClient | null => supabaseClientRef.value

/**
 * Gets the current backend ID
 */
export const getCurrentBackendId = (): string | null => currentBackendIdRef.value

/**
 * Initializes Supabase client for a specific backend
 * Reuses existing client if already initialized for the same backend
 */
export const initSupabaseClientAsync = async (
  backendId: string,
  serverUrl: string,
): Promise<void> => {
  // If client already exists for this backend, reuse it
  if (supabaseClientRef.value && currentBackendIdRef.value === backendId) {
    return
  }

  // Clean up existing client before creating a new one
  // Prevents "Multiple GoTrueClient instances" on Android WebView reloads
  if (supabaseClientRef.value) {
    log.info('Cleaning up existing Supabase client before creating new one')
    try {
      supabaseClientRef.value.realtime.removeAllChannels()
      supabaseClientRef.value.realtime.disconnect()
    } catch (e) {
      log.warn('Failed to clean up existing Supabase client:', e)
    }
    supabaseClientRef.value = null
    currentBackendIdRef.value = null
  }

  // Get Supabase URL and anon key from server health check
  const response = await fetch(serverUrl)
  if (!response.ok) {
    throw new Error('Failed to connect to sync server')
  }

  const serverInfo = await response.json()
  const supabaseUrl = serverInfo.supabaseUrl
  const supabaseAnonKey = serverInfo.supabaseAnonKey

  if (!supabaseUrl || !supabaseAnonKey) {
    throw new Error('Supabase configuration missing from server')
  }

  // Create new client
  const client = createClient(supabaseUrl, supabaseAnonKey, {
    auth: {
      // Use backend-specific storage key to avoid conflicts
      storageKey: `sb-${backendId}-auth-token`,
      // Tauri is a single WebView context — no URL-based auth flow
      detectSessionInUrl: false,
    },
    realtime: {
      // Increase timeout for mobile connections (default is 10s)
      timeout: 30000,
      // Heartbeat interval to keep connection alive on mobile
      heartbeatIntervalMs: 15000,
    },
  })
  supabaseClientRef.value = client as AppSupabaseClient
  currentBackendIdRef.value = backendId

  // Listen for auth state changes to keep realtime connection authenticated
  // This is critical: when the token refreshes, we must update the realtime connection
  client.auth.onAuthStateChange((event, session) => {
    if (session?.access_token) {
      cachedAccessToken = session.access_token
      invoke('set_auth_token', { token: session.access_token }).catch(() => {})
    }
    if (event === 'TOKEN_REFRESHED' && session?.access_token) {
      log.info('Auth token refreshed, updating realtime connection')
      supabaseClientRef.value?.realtime.setAuth(session.access_token)
    } else if (event === 'SIGNED_OUT') {
      log.info('User signed out, realtime will disconnect')
      cachedAccessToken = null
      invoke('set_auth_token', { token: null }).catch(() => {})
    }
  })
}

/**
 * Registers a callback that resolves the DID auth context for the current backend.
 * Called by the sync engine after initialization so that getAuthTokenAsync can
 * automatically re-authenticate via DID challenge when refresh tokens are invalid.
 */
export const setReauthResolver = (resolver: ReauthContextResolver | null): void => {
  reauthResolver = resolver
}

/**
 * Performs DID-based authentication against the sync server.
 * Returns { access_token, refresh_token } on success.
 */
const didAuthenticateAsync = async (
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
    { name: 'ECDSA', hash: 'SHA-256' },
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
 * Attempts to re-authenticate via DID challenge when refresh token is invalid.
 * Returns the new access token on success, null on failure.
 */
const attemptDidReauthAsync = async (): Promise<string | null> => {
  if (!reauthResolver || !supabaseClientRef.value || reauthInProgress) return null

  reauthInProgress = true
  try {
    const ctx = await reauthResolver()
    if (!ctx) {
      log.warn('DID re-auth: no context available (vault not open?)')
      return null
    }

    log.info('DID re-auth: refresh token invalid, re-authenticating via DID challenge...')
    const session = await didAuthenticateAsync(ctx.serverUrl, ctx.did, ctx.privateKey)

    const { error } = await supabaseClientRef.value.auth.setSession({
      access_token: session.access_token,
      refresh_token: session.refresh_token,
    })

    if (error) {
      log.error('DID re-auth: setSession failed:', error.message)
      return null
    }

    cachedAccessToken = session.access_token
    invoke('set_auth_token', { token: session.access_token }).catch(() => {})
    supabaseClientRef.value.realtime.setAuth(session.access_token)
    log.info('DID re-auth: successfully re-authenticated')
    return session.access_token
  } catch (e) {
    log.error('DID re-auth failed:', e)
    return null
  } finally {
    reauthInProgress = false
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
 * Gets the current Supabase auth token, automatically refreshing if expired
 */
export const getAuthTokenAsync = async (): Promise<string | null> => {
  if (!supabaseClientRef.value) {
    return cachedAccessToken
  }

  const {
    data: { session },
  } = await supabaseClientRef.value.auth.getSession()
  let token = session?.access_token ?? cachedAccessToken

  // Proactively refresh if token is expired or about to expire
  if (token && isTokenExpired(token)) {
    log.info('Auth token expired, refreshing...')
    const { data, error } = await supabaseClientRef.value.auth.refreshSession()
    if (error) {
      // Any refresh failure means the session is invalid — attempt DID re-auth
      const errorCode = (error as { code?: string }).code
      log.error(`Auth refresh failed (code=${errorCode}): ${error.message} — attempting DID re-authentication...`)
      const newToken = await attemptDidReauthAsync()
      if (newToken) return newToken
      cachedAccessToken = null
      return null
    }
    if (data.session?.access_token) {
      token = data.session.access_token
      cachedAccessToken = token
      invoke('set_auth_token', { token }).catch(() => {})
      supabaseClientRef.value?.realtime.setAuth(token)
      log.info('Auth token refreshed successfully')
    }
  }

  if (token) {
    cachedAccessToken = token
  }
  return token
}

/**
 * Caches an access token directly (workaround for Supabase getSession timing issues)
 */
export const cacheAccessToken = (token: string): void => {
  cachedAccessToken = token
}

/**
 * Sets an existing Supabase client (for cases where client is created externally, e.g., connect wizard)
 * This is used when the client is already authenticated and we want to reuse it
 */
export const setSupabaseClient = (
  client: AppSupabaseClient,
  backendId: string,
): void => {
  supabaseClientRef.value = client
  currentBackendIdRef.value = backendId

  // Listen for auth state changes to keep realtime connection authenticated
  client.auth.onAuthStateChange((event, session) => {
    if (session?.access_token) {
      cachedAccessToken = session.access_token
      invoke('set_auth_token', { token: session.access_token }).catch(() => {})
    }
    if (event === 'TOKEN_REFRESHED' && session?.access_token) {
      log.info('Auth token refreshed, updating realtime connection')
      supabaseClientRef.value?.realtime.setAuth(session.access_token)
    } else if (event === 'SIGNED_OUT') {
      log.info('User signed out, realtime will disconnect')
      cachedAccessToken = null
      invoke('set_auth_token', { token: null }).catch(() => {})
    }
  })
}

/**
 * Resets the Supabase client state
 */
export const resetSupabaseClient = (): void => {
  if (supabaseClientRef.value) {
    try {
      supabaseClientRef.value.realtime.removeAllChannels()
      supabaseClientRef.value.realtime.disconnect()
    } catch {
      // Ignore cleanup errors during reset
    }
  }
  supabaseClientRef.value = null
  currentBackendIdRef.value = null
  cachedAccessToken = null
  invoke('set_auth_token', { token: null }).catch(() => {})
}
