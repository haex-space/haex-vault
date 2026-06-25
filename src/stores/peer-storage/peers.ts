import { invoke } from '@tauri-apps/api/core'
import type { Ref } from 'vue'
import { decodeUcan, type Capability } from '@haex-space/ucan'
import { createLogger } from '@/stores/logging'
import type { FileEntry } from '~/../src-tauri/bindings/FileEntry'
import type { SelectHaexPeerShares, SelectHaexSpaceDevices } from '~/database/schemas'
import { getUcanForSpaceAsync } from '~/utils/auth/ucanStore'
import type { CreateTransferChannel } from './transfers'

const log = createLogger('PEER_STORAGE')

export interface PeersContext {
  shares: Ref<SelectHaexPeerShares[]>
  spaceDevices: Ref<SelectHaexSpaceDevices[]>
  acceptedInviteEndpoints: Ref<Array<{ spaceId: string, endpointId: string }>>
  activeTransfers: Ref<number>
  createTransferChannel: CreateTransferChannel
}

export const createPeersModule = (ctx: PeersContext) => {
  const { shares, spaceDevices, acceptedInviteEndpoints, activeTransfers, createTransferChannel } = ctx

  // Resolve which space a remote request belongs to, so the matching UCAN
  // can be picked. The first path segment is the share name; the share row
  // (replicated via CRDT) carries the authoritative spaceId.
  //
  // `spaceIdHint` is used when the caller already knows the authoritative
  // spaceId (e.g. an entry from `remoteListAllSharesAsync` that knows its
  // origin space). This bypasses the by-name lookup, which is otherwise
  // ambiguous when a single peer hosts shares with identical names in
  // different spaces. Without the hint we sort matching shares/devices by
  // spaceId so at least the picked space is stable across calls.
  //
  // Root-path lookups (path='/') depend on the inviter's haex_space_devices
  // row, which only lands after the CRDT pull following accept. Without a
  // fallback, clicking the file browser between accept-complete and
  // CRDT-row-arrived throws "No valid UCAN token". We can't seed
  // haex_space_devices ourselves: the synthetic deviceId we'd have to pass
  // (the invite payload doesn't carry the inviter's real one) fires
  // haex_space_devices_ensure_refs and creates a haex_devices stub claiming
  // the inviter's endpoint_id. That stub then blocks any later peer_shares
  // CRDT row (whose ensure-refs trigger silently fails the INSERT OR IGNORE
  // on haex_devices_endpoint_id_unique, leaving peer_shares.device_id
  // dangling). Instead, fall back to (spaceId, endpoint) tuples extracted
  // from accepted invites — same information, no schema corruption.
  const resolveRequestContext = (
    remoteNodeId: string,
    path: string,
    spaceIdHint?: string,
  ) => {
    const trimmed = path.replace(/^\/+/, '')
    const shareName = trimmed.split('/')[0]
    let matchingShare: SelectHaexPeerShares | undefined
    if (spaceIdHint && shareName) {
      matchingShare = shares.value.find(
        s => s.endpointId === remoteNodeId
          && s.name === shareName
          && s.spaceId === spaceIdHint,
      )
    } else if (shareName) {
      const candidates = shares.value
        .filter(s => s.endpointId === remoteNodeId && s.name === shareName)
        .sort((a, b) => a.spaceId.localeCompare(b.spaceId))
      matchingShare = candidates[0]
    }
    if (shareName && !matchingShare) {
      return { ucanToken: null, relayUrl: null }
    }
    const resolvedSpaceId = matchingShare?.spaceId ?? spaceIdHint
    const deviceCandidates = spaceDevices.value
      .filter(d => d.endpointId === remoteNodeId
        && (resolvedSpaceId ? d.spaceId === resolvedSpaceId : true))
      .sort((a, b) => a.spaceId.localeCompare(b.spaceId))
    const device = deviceCandidates[0]
    let spaceId = resolvedSpaceId ?? device?.spaceId
    if (!spaceId) {
      const inviteTuple = acceptedInviteEndpoints.value.find(
        t => t.endpointId === remoteNodeId,
      )
      spaceId = inviteTuple?.spaceId
    }
    const ucanToken = spaceId ? getUcanForSpaceAsync(spaceId) : null
    return { ucanToken, relayUrl: device?.relayUrl ?? null }
  }

  const getCapabilityForPeer = (
    remoteNodeId: string,
    path: string,
    spaceIdHint?: string,
  ): Capability | null => {
    const { ucanToken } = resolveRequestContext(remoteNodeId, path, spaceIdHint)
    if (!ucanToken) return null
    try {
      const decoded = decodeUcan(ucanToken)
      const caps = decoded.payload.cap as Record<string, Capability>
      return Object.values(caps)[0] ?? null
    } catch {
      return null
    }
  }

  const remoteListAsync = async (
    remoteNodeId: string,
    path: string,
    spaceIdHint?: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, path, spaceIdHint,
    )
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    activeTransfers.value++
    try {
      return await invoke<FileEntry[]>('peer_storage_remote_list', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path,
        ucanToken,
      })
    } finally {
      activeTransfers.value--
    }
  }

  // Root listing across all shared spaces with a peer. `remoteListAsync` with
  // path='/' can only see ONE space's shares: the leader enforces a Layer-1.5
  // narrowing of effective_spaces = UCAN.capabilities ∩ allowed_peers, so the
  // returned set is filtered to the single space whose UCAN was sent. When a
  // peer shares multiple spaces with us, the file-browser-root view would
  // otherwise show only one space's shares (the one picked by the FIRST
  // device row match in resolveRequestContext, which is non-deterministic and
  // also leaks names across runs). This fans out one parallel request per
  // space we share with the peer, each scoped to that space's UCAN, and
  // tags each returned entry with its origin spaceId so downstream calls
  // can address the right space when share names collide across spaces.
  const remoteListAllSharesAsync = async (
    remoteNodeId: string,
  ): Promise<Array<FileEntry & { spaceId: string }>> => {
    const peerSpaceIds = [...new Set(
      spaceDevices.value
        .filter(d => d.endpointId === remoteNodeId)
        .map(d => d.spaceId),
    )]

    if (peerSpaceIds.length === 0) return []

    const fetchOneSpace = async (
      spaceId: string,
    ): Promise<Array<FileEntry & { spaceId: string }>> => {
      const ucanToken = getUcanForSpaceAsync(spaceId)
      if (!ucanToken) {
        log.warn(`remoteListAllSharesAsync: skipping space ${spaceId.slice(0, 8)} — no cached UCAN`)
        return []
      }
      const device = spaceDevices.value.find(
        d => d.endpointId === remoteNodeId && d.spaceId === spaceId,
      )
      activeTransfers.value++
      try {
        const entries = await invoke<FileEntry[]>('peer_storage_remote_list', {
          nodeId: remoteNodeId,
          relayUrl: device?.relayUrl ?? null,
          path: '/',
          ucanToken,
        })
        return entries.map(entry => ({ ...entry, spaceId }))
      } catch (err) {
        // Re-throw so the caller can surface this to the user if all spaces fail.
        throw new Error(`space ${spaceId.slice(0, 8)}: ${err}`)
      } finally {
        activeTransfers.value--
      }
    }

    const settled = await Promise.allSettled(peerSpaceIds.map(fetchOneSpace))
    const fulfilled = settled.filter(
      (r): r is PromiseFulfilledResult<Array<FileEntry & { spaceId: string }>> =>
        r.status === 'fulfilled',
    )
    const succeeded = fulfilled.flatMap(r => r.value)
    const failures = settled.filter((r): r is PromiseRejectedResult => r.status === 'rejected')

    if (fulfilled.length === 0 && failures.length > 0) {
      // Every attempted space failed to connect — throw the first connection
      // error so the file browser can show a meaningful message.
      throw failures[0]!.reason
    }

    return succeeded
  }

  const remoteReadAsync = async (
    remoteNodeId: string,
    path: string,
    saveTo?: string,
    spaceIdHint?: string,
    expectedSize?: number | bigint,
    expectedModified?: number | bigint | null,
    spaceFolder?: string | null,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, path, spaceIdHint,
    )
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    const transferId = crypto.randomUUID()
    const { channel, promise } = createTransferChannel(transferId, path, 'download')

    activeTransfers.value++
    try {
      await invoke<string>('peer_storage_remote_read', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path,
        transferId,
        saveTo: saveTo ?? null,
        expectedSize: expectedSize == null ? null : Number(expectedSize),
        expectedModified: expectedModified == null ? null : Number(expectedModified),
        spaceFolder: spaceFolder ?? null,
        spaceId: spaceIdHint ?? null,
        ucanToken,
        onEvent: channel,
      })

      return await promise
    } finally {
      activeTransfers.value--
    }
  }

  const remoteWriteAsync = async (
    remoteNodeId: string,
    remotePath: string,
    sourcePath: string,
    spaceIdHint?: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, remotePath, spaceIdHint,
    )
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')

    const transferId = crypto.randomUUID()
    const { channel, promise } = createTransferChannel(transferId, remotePath, 'upload')

    activeTransfers.value++
    try {
      await invoke('peer_storage_remote_write', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path: remotePath,
        sourcePath,
        transferId,
        ucanToken,
        onEvent: channel,
      })

      await promise
    } finally {
      activeTransfers.value--
    }
  }

  const remoteCreateDirectoryAsync = async (
    remoteNodeId: string,
    remotePath: string,
    spaceIdHint?: string,
  ) => {
    const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(
      remoteNodeId, remotePath, spaceIdHint,
    )
    if (!ucanToken) throw new Error('No valid UCAN token for this peer\'s space')
    await invoke('peer_storage_remote_create_directory', {
      nodeId: remoteNodeId,
      relayUrl: deviceRelayUrl,
      path: remotePath,
      ucanToken,
    })
  }

  const checkPeerOnlineAsync = async (remoteNodeId: string): Promise<boolean> => {
    try {
      const { ucanToken, relayUrl: deviceRelayUrl } = resolveRequestContext(remoteNodeId, '/')
      if (!ucanToken) return false
      await invoke<FileEntry[]>('peer_storage_remote_list', {
        nodeId: remoteNodeId,
        relayUrl: deviceRelayUrl,
        path: '/',
        ucanToken,
      })
      return true
    } catch {
      return false
    }
  }

  return {
    resolveRequestContext,
    getCapabilityForPeer,
    remoteListAsync,
    remoteListAllSharesAsync,
    remoteReadAsync,
    remoteWriteAsync,
    remoteCreateDirectoryAsync,
    checkPeerOnlineAsync,
  }
}
