import { invoke } from '@tauri-apps/api/core'

/**
 * Wire representation of a preference scope. Vault-scoped rows are
 * visible on every device; device-scoped rows apply only to their
 * device UUID.
 */
export type PrefScope = { kind: 'vault' } | { kind: 'device'; uuid: string }

/**
 * Which fallback branch the backend resolver picked. See spec 002
 * §FR-014.
 */
export type ResolveSource =
  | 'session'
  | 'last_active'
  | 'default_device'
  | 'default_vault'
  | 'first_available'
  | 'none'

export interface ResolveDefaultModelResult {
  modelId: string | null
  source: ResolveSource
}

/**
 * Delegate autonomy posture (spec 009-autonomous-delegate-mode), stored as
 * the vault-scoped `chat.autonomy_mode` preference. Shared here — rather
 * than each reader re-declaring the literal union and hand-rolling its own
 * `=== 'standard' || === 'ungated' || === 'gated_permissive'` validation —
 * so `AutonomyModeSetting.vue` (the writer) and `[instance].vue` (a reader)
 * can't drift out of sync if a mode is ever added or renamed.
 */
export const AUTONOMY_MODES = [
  'standard',
  'ungated',
  'gated_permissive',
] as const
export type AutonomyMode = (typeof AUTONOMY_MODES)[number]

export function isAutonomyMode(value: unknown): value is AutonomyMode {
  return (
    typeof value === 'string' &&
    (AUTONOMY_MODES as readonly string[]).includes(value)
  )
}

/**
 * Namespaced key/value preferences per device or vault-wide, plus the
 * session-start resolver. Values are opaque strings — for model
 * defaults the value is a model-id (composite `<uuid>:<remote>` for
 * api_key providers, catalog slug for local ones).
 */
export function usePreferences() {
  async function getPrefAsync(
    scope: PrefScope,
    key: string,
  ): Promise<string | null> {
    return await invoke<string | null>('get_pref', { args: { scope, key } })
  }

  async function setPrefAsync(
    scope: PrefScope,
    key: string,
    value: string,
  ): Promise<void> {
    await invoke('set_pref', { args: { scope, key, value } })
  }

  async function clearPrefAsync(scope: PrefScope, key: string): Promise<void> {
    await invoke('clear_pref', { args: { scope, key } })
  }

  async function resolveDefaultModelAsync(): Promise<ResolveDefaultModelResult> {
    return await invoke<ResolveDefaultModelResult>('resolve_default_model')
  }

  return {
    getPrefAsync,
    setPrefAsync,
    clearPrefAsync,
    resolveDefaultModelAsync,
  }
}
