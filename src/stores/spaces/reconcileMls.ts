/**
 * Leader-side MLS reconciliation.
 *
 * When a member self-leaves (via `removeSelfFromSpace`) the propagated CRDT
 * delete-log entry shows up on every peer's `haex_space_members` table. The
 * leader is then responsible for rotating the MLS group key (forward
 * secrecy) and broadcasting the commit so all remaining peers re-derive
 * their epoch key.
 *
 * This module compares the *previous* set of member DIDs (snapshot) with
 * the current set per local space. Disappearing DIDs trigger the same
 * MLS+broadcast sequence the admin-kick path uses (`removeSpaceMember` in
 * `members.ts`), but skips the DB delete because it has already happened.
 *
 * Scope: only local (P2P) spaces — for online spaces the home server is the
 * authoritative leader and rotates MLS server-side.
 */

import { invoke } from '@tauri-apps/api/core'
import { eq } from 'drizzle-orm'
import {
  haexIdentities,
  haexSpaceMembers,
} from '~/database/schemas'
import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'
import type { schema } from '~/database'
import { createLogger } from '@/stores/logging'
import { SpaceType, SpaceStatus } from '~/database/constants'
import type { ElectionResultInfo } from '@bindings/ElectionResultInfo'
import type { SpaceWithType } from './index'

type DB = SqliteRemoteDatabase<typeof schema>
const log = createLogger('SPACES:MLS-RECONCILE')

/**
 * Per-space snapshot of the last seen member-DID set. Lives at module scope
 * so it survives across reconciliation cycles for the lifetime of the vault
 * session. Reset by `resetMemberSnapshot` (e.g. on vault close).
 */
const memberSnapshots: Map<string, Set<string>> = new Map()

export function resetMemberSnapshots(): void {
  memberSnapshots.clear()
}

async function fetchMemberDidsAsync(db: DB, spaceId: string): Promise<Set<string>> {
  const rows = await db
    .select({ did: haexIdentities.did })
    .from(haexSpaceMembers)
    .innerJoin(haexIdentities, eq(haexSpaceMembers.identityId, haexIdentities.id))
    .where(eq(haexSpaceMembers.spaceId, spaceId))
  return new Set(rows.map((r) => r.did))
}

async function isLeaderForSpaceAsync(spaceId: string): Promise<boolean> {
  try {
    const election = await invoke<ElectionResultInfo>('local_delivery_elect', { spaceId })
    return election.role === 'leader'
  } catch (error) {
    log.warn(`Election check for space ${spaceId} failed: ${error}`)
    return false
  }
}

async function rekeyMlsForRemovedMemberAsync(
  spaceId: string,
  removedDid: string,
): Promise<void> {
  const memberIndex = await invoke<number | null>('mls_find_member_index', {
    spaceId,
    memberDid: removedDid,
  })
  if (memberIndex === null) {
    // MLS already doesn't know about this DID — either we already rotated,
    // or the member never managed to join the group. Nothing to do.
    log.debug(
      `Skipping MLS rekey for ${removedDid.slice(0, 16)}…: not in MLS group ${spaceId}`,
    )
    return
  }

  const bundle = await invoke<{
    commit: number[]
    welcome: number[] | null
    groupInfo: number[]
  }>('mls_remove_member', { spaceId, memberIndex })

  if (bundle.commit.length > 0) {
    try {
      await invoke('local_delivery_broadcast_commit', {
        spaceId,
        commit: bundle.commit,
      })
    } catch (error) {
      // Non-fatal: leader is offline or no peers connected. The commit is
      // valid; remaining peers will pick it up on next reconnect via
      // buffered messages.
      log.warn(
        `Broadcast of MLS rekey commit failed for ${spaceId}: ${error}`,
      )
    }
  }

  // Forward secrecy: derive new epoch key after the removal commit.
  await invoke('mls_export_epoch_key', { spaceId })

  log.info(
    `MLS rekey complete for space ${spaceId} (removed ${removedDid.slice(0, 16)}…)`,
  )
}

/**
 * Inspect every local active space we are leader of, diff member DIDs
 * against the last snapshot, and trigger MLS rekey for each disappeared
 * DID. Safe to call concurrently with itself thanks to the per-space async
 * sequencing inside; double-trigger is idempotent because
 * `mls_find_member_index` returns null after the first rotation.
 */
export async function reconcileMlsAfterMemberSyncAsync(
  db: DB,
  spaces: ReadonlyArray<SpaceWithType>,
): Promise<void> {
  for (const space of spaces) {
    if (space.type !== SpaceType.LOCAL) continue
    if (space.status !== SpaceStatus.ACTIVE) continue

    const isFirstPass = !memberSnapshots.has(space.id)
    const current = await fetchMemberDidsAsync(db, space.id)
    const previous = memberSnapshots.get(space.id) ?? new Set<string>()

    // Update snapshot first — even if rekey fails, we don't want to retry
    // the same disappeared DIDs forever; MLS handles its own idempotency.
    memberSnapshots.set(space.id, current)

    // First pass for this space → just prime the snapshot, no diff. Without
    // this, a freshly-loaded space would diff against an empty Set and we'd
    // never detect *real* future removals correctly on subsequent passes.
    if (isFirstPass) continue

    const removed: string[] = []
    for (const did of previous) {
      if (!current.has(did)) removed.push(did)
    }
    if (removed.length === 0) continue

    if (!(await isLeaderForSpaceAsync(space.id))) {
      log.debug(
        `Detected ${removed.length} member removal(s) in ${space.id} but I am not leader — skipping rekey`,
      )
      continue
    }

    log.info(
      `Leader-side MLS reconcile: ${removed.length} removed member(s) in space ${space.id}`,
    )
    for (const did of removed) {
      try {
        await rekeyMlsForRemovedMemberAsync(space.id, did)
      } catch (error) {
        log.warn(
          `MLS rekey for ${did.slice(0, 16)}… in ${space.id} failed: ${error}`,
        )
      }
    }
  }
}
