/**
 * Pure helpers for the streaming pull-and-apply pipeline.
 *
 * Lives in its own module (no Nuxt auto-imports, no Pinia stores, no Tauri
 * invokes) so it's unit-testable from vitest without pulling in the broadcast
 * / extension-handler chain that the orchestrator-level modules require.
 */

import type { ColumnChange } from '../tableScanner'
import { hlcIsNewer } from '@/utils/hlc'

/**
 * Splits a buffer of pulled changes into "apply now" and "hold back" subsets,
 * mirroring the P2P sync loop's `split_complete_groups` (Rust). HLC == one
 * source transaction; when more pages are coming, the trailing (max-HLC) group
 * in the buffer may still be partially delivered, so it is held back until a
 * later page — or `hasMore = false` — confirms it is complete. This guarantees
 * a source transaction is never applied in pieces.
 *
 * Pure; deterministic; no I/O.
 */
export function splitCompleteGroups(
  changes: ColumnChange[],
  hasMore: boolean,
): { toApply: ColumnChange[]; holdBack: ColumnChange[] } {
  if (changes.length === 0) return { toApply: [], holdBack: [] }
  if (!hasMore) return { toApply: changes, holdBack: [] }
  let maxHlc = changes[0]!.hlcTimestamp
  for (let i = 1; i < changes.length; i++) {
    if (hlcIsNewer(changes[i]!.hlcTimestamp, maxHlc)) maxHlc = changes[i]!.hlcTimestamp
  }
  const toApply: ColumnChange[] = []
  const holdBack: ColumnChange[] = []
  for (const c of changes) {
    if (c.hlcTimestamp === maxHlc) holdBack.push(c)
    else toApply.push(c)
  }
  return { toApply, holdBack }
}

/**
 * Groups changes by their `hlcTimestamp` (== one source transaction) in
 * ascending HLC order. Used to apply each source transaction as its OWN DB
 * transaction on the receiver, so the 100 MB per-transaction cap (ADR 0001) is
 * never crossed by the receiving apply path even when the cumulative pull
 * delta is larger.
 *
 * Pure; deterministic; no I/O.
 */
export function groupByHlcAscending(
  changes: ColumnChange[],
): Array<{ hlc: string; changes: ColumnChange[] }> {
  const buckets = new Map<string, ColumnChange[]>()
  for (const c of changes) {
    const bucket = buckets.get(c.hlcTimestamp)
    if (bucket) bucket.push(c)
    else buckets.set(c.hlcTimestamp, [c])
  }
  return [...buckets.entries()]
    .sort(([a], [b]) => (hlcIsNewer(a, b) ? 1 : a === b ? 0 : -1))
    .map(([hlc, list]) => ({ hlc, changes: list }))
}
