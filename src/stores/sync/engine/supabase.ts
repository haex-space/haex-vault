/**
 * Supabase Client Management
 * Handles Supabase client initialization and authentication
 */

import { shallowRef } from 'vue'
import { createClient } from '@supabase/supabase-js'
import { engineLog as log } from './types'

// Use the actual return type of createClient for consistency across the codebase
export type AppSupabaseClient = ReturnType<typeof createClient>

// Module state — using shallowRef so Vue computed() can track changes
export const supabaseClientRef = shallowRef<AppSupabaseClient | null>(null)
export const currentBackendIdRef = shallowRef<string | null>(null)
let cachedAccessToken: string | null = null

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
    }
    if (event === 'TOKEN_REFRESHED' && session?.access_token) {
      log.info('Auth token refreshed, updating realtime connection')
      supabaseClientRef.value?.realtime.setAuth(session.access_token)
    } else if (event === 'SIGNED_OUT') {
      log.info('User signed out, realtime will disconnect')
      cachedAccessToken = null
    }
  })
}

/**
 * Gets the current Supabase auth token
 */
export const getAuthTokenAsync = async (): Promise<string | null> => {
  if (!supabaseClientRef.value) {
    return cachedAccessToken
  }

  const {
    data: { session },
  } = await supabaseClientRef.value.auth.getSession()
  const token = session?.access_token ?? cachedAccessToken
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
    }
    if (event === 'TOKEN_REFRESHED' && session?.access_token) {
      log.info('Auth token refreshed, updating realtime connection')
      supabaseClientRef.value?.realtime.setAuth(session.access_token)
    } else if (event === 'SIGNED_OUT') {
      log.info('User signed out, realtime will disconnect')
      cachedAccessToken = null
    }
  })
}

/**
 * Resets the Supabase client state
 */
export const resetSupabaseClient = (): void => {
  supabaseClientRef.value = null
  currentBackendIdRef.value = null
  cachedAccessToken = null
}
