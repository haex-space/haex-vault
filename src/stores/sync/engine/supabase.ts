/**
 * Supabase Client Management
 * Handles Supabase client initialization and authentication
 */

import { createClient, type SupabaseClient } from '@supabase/supabase-js'
import { log } from './types'

// Module state
let supabaseClient: SupabaseClient | null = null
let currentBackendId: string | null = null

/**
 * Gets the current Supabase client
 */
export const getSupabaseClient = (): SupabaseClient | null => supabaseClient

/**
 * Gets the current backend ID
 */
export const getCurrentBackendId = (): string | null => currentBackendId

/**
 * Initializes Supabase client for a specific backend
 * Reuses existing client if already initialized for the same backend
 */
export const initSupabaseClientAsync = async (
  backendId: string,
  serverUrl: string,
): Promise<void> => {
  // If client already exists for this backend, reuse it
  if (supabaseClient && currentBackendId === backendId) {
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
  supabaseClient = createClient(supabaseUrl, supabaseAnonKey, {
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
  currentBackendId = backendId

  // Listen for auth state changes to keep realtime connection authenticated
  // This is critical: when the token refreshes, we must update the realtime connection
  supabaseClient.auth.onAuthStateChange((event, session) => {
    if (event === 'TOKEN_REFRESHED' && session?.access_token) {
      log.info('Auth token refreshed, updating realtime connection')
      supabaseClient?.realtime.setAuth(session.access_token)
    } else if (event === 'SIGNED_OUT') {
      log.info('User signed out, realtime will disconnect')
    }
  })
}

/**
 * Gets the current Supabase auth token
 */
export const getAuthTokenAsync = async (): Promise<string | null> => {
  if (!supabaseClient) {
    return null
  }

  const {
    data: { session },
  } = await supabaseClient.auth.getSession()
  return session?.access_token ?? null
}

/**
 * Resets the Supabase client state
 */
export const resetSupabaseClient = (): void => {
  supabaseClient = null
  currentBackendId = null
}
