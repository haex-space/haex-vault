import { invoke } from '@tauri-apps/api/core'
import type { ElectionResultInfo } from '@bindings/ElectionResultInfo'
import type { SelectHaexSpaces } from '~/database/schemas'
import { SpaceType, SpaceStatus } from '~/database/constants'
import { createLogger } from '@/stores/logging'

const log = createLogger('SPACES:PEER_SYNC')

export async function startLocalSpaceLeaders(spaces: SelectHaexSpaces[]) {
  for (const space of spaces) {
    if (
      space.type === SpaceType.LOCAL &&
      space.status === SpaceStatus.ACTIVE
    ) {
      try {
        await invoke('local_delivery_start', {
          spaceId: space.id,
        })
        log.info(
          `Started leader mode for local space ${space.id}`,
        )
      } catch {
        // Already running — ignore
      }
    }
  }
}

/**
 * Start a peer sync loop for a single local space after running leader
 * election. If a `hintLeaderEndpointId` is provided and election does not
 * find a leader, falls back to the hint — useful right after an invite
 * Accept, where we know which endpoint served the ClaimInvite but election
 * may not yet have the fresh space devices registered.
 */
export async function startPeerSyncForLocalSpace(
  spaceId: string,
  identityDid: string,
  hintLeaderEndpointId?: string,
  hintLeaderRelayUrl?: string | null,
): Promise<void> {
  let leaderEndpointId: string | undefined
  let leaderRelayUrl: string | null | undefined
  try {
    const election = await invoke<ElectionResultInfo>(
      'local_delivery_elect',
      { spaceId },
    )
    if (election.role === 'leader') {
      log.debug(`Space ${spaceId}: self is leader, no peer sync needed`)
      return
    }
    if (election.role === 'peer' && election.leaderEndpointId) {
      leaderEndpointId = election.leaderEndpointId
      leaderRelayUrl = election.leaderRelayUrl
    } else {
      log.debug(`Space ${spaceId}: no leader found via election (role=${election.role})`)
    }
  } catch (error) {
    log.warn(`Election for space ${spaceId} failed: ${error}`)
  }

  if (!leaderEndpointId && hintLeaderEndpointId) {
    leaderEndpointId = hintLeaderEndpointId
    leaderRelayUrl = hintLeaderRelayUrl ?? null
    log.info(`Space ${spaceId}: using hint endpoint as leader (${hintLeaderEndpointId.slice(0, 16)})`)
  }

  if (!leaderEndpointId) return

  // UCAN is resolved inside Rust from haex_ucan_tokens at connect/reconnect
  // time — no token to pass from the frontend.
  try {
    await invoke('local_delivery_connect', {
      spaceId,
      leaderEndpointId,
      leaderRelayUrl: leaderRelayUrl ?? null,
      identityDid,
    })
    log.info(`Started peer sync for space ${spaceId} → leader ${leaderEndpointId.slice(0, 16)}`)
  } catch (error) {
    // Already connected, or temporarily unreachable — non-fatal.
    log.debug(`Peer sync connect for ${spaceId}: ${error}`)
  }
}

/**
 * For every joined local space, run leader election and — if another
 * device is the elected leader — start a peer sync loop against them.
 *
 * Without this, an invitee-side vault accepts the MLS welcome but never
 * pulls CRDT history (peer_shares, other members, space_devices), so the
 * space appears mostly empty after joining.
 *
 * Idempotent: `local_delivery_connect` errors if a loop is already
 * running for the space — we swallow that case.
 */
export async function startLocalSpacePeerSync(
  spaces: SelectHaexSpaces[],
  myIdentityDid: string | undefined,
) {
  if (!myIdentityDid) {
    log.warn('Peer sync skipped: no own identity')
    return
  }

  for (const space of spaces) {
    // ACTIVE spaces sync normally. LEAVING spaces also need peer-sync
    // running so their pending delete-log entries can be pushed to the
    // leader the next time it is reachable; without this the offline-leave
    // resilience would never have a transport to flush over.
    const wantsPeerSync =
      space.type === SpaceType.LOCAL &&
      (space.status === SpaceStatus.ACTIVE
        || space.status === SpaceStatus.LEAVING)
    if (!wantsPeerSync) continue

    await startPeerSyncForLocalSpace(
      space.id,
      myIdentityDid,
    ).catch((error) => {
      log.warn(`Peer sync for space ${space.id} failed: ${error}`)
    })
  }
}
