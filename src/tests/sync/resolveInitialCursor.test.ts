import { describe, expect, it, vi } from 'vitest'

// cursor.ts (where resolveInitialCursor lives) transitively imports modules
// that require a Tauri runtime or Nuxt auto-imports.  Stub them all so the
// module resolves cleanly under vitest/jsdom without touching any runtime.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn() }))
vi.mock('@tauri-apps/plugin-http', () => ({ fetch: vi.fn() }))
vi.mock('~/stores/extensions/broadcast', () => ({
  useExtensionBroadcastStore: () => ({
    broadcastSyncTablesUpdated: vi.fn(),
  }),
}))

import { resolveInitialCursor } from '@/stores/sync/orchestrator/pull/cursor'

describe('resolveInitialCursor', () => {
  it('returns the persisted cursor unchanged when no pending tables exist', () => {
    const result = resolveInitialCursor('2024-01-01T00:00:00Z', [])
    expect(result.cursor).toBe('2024-01-01T00:00:00Z')
    expect(result.recoveredTables).toEqual([])
  })

  it('returns null cursor when recoverable pending tables exist', () => {
    const result = resolveInitialCursor('2024-01-01T00:00:00Z', ['haex_some_table', 'haex_other_table'])
    expect(result.cursor).toBeNull()
    expect(result.recoveredTables).toEqual(['haex_some_table', 'haex_other_table'])
  })

  it('returns null cursor when persisted cursor is null and pending tables exist', () => {
    const result = resolveInitialCursor(null, ['haex_some_table'])
    expect(result.cursor).toBeNull()
    expect(result.recoveredTables).toEqual(['haex_some_table'])
  })

  it('returns null cursor unchanged when no pending tables and persisted cursor is null', () => {
    const result = resolveInitialCursor(null, [])
    expect(result.cursor).toBeNull()
    expect(result.recoveredTables).toEqual([])
  })
})
