import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { and, eq } from 'drizzle-orm'
import type { Ref } from 'vue'
import { RUST_EVENTS, type PeerStorageStateEvent } from '@/lib/rust-events'
import { createOnceListener, type OnceListener } from '@/lib/once-listener'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'
import type { PeerStorageStartInfo } from '~/../src-tauri/bindings/PeerStorageStartInfo'
import type { PeerStorageStatus } from '~/../src-tauri/bindings/PeerStorageStatus'
import { haexSpaceDevices, haexVaultSettings } from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'
import { ownerSyncAutostartEnabled } from './owner-sync'

const log = createLogger('PEER_STORAGE')

export interface LifecycleContext {
  running: Ref<boolean>
  nodeId: Ref<string>
  relayUrl: Ref<string | null>
  configuredRelayUrl: Ref<string | null>
  ownerSyncRunning: Ref<boolean>
  loadConfiguredRelayUrlAsync: () => Promise<void>
  loadSpaceDevicesAsync: () => Promise<void>
  loadAcceptedInviteEndpointsAsync: () => Promise<void>
  startOwnerSyncAsync: () => Promise<void>
  stopOwnerSyncAsync: () => Promise<void>
  // Callback to restart the endpoint after Rust-side close. Wired by index.ts
  // to the public `startAsync` so the in-flight promise / dedup logic is honoured.
  requestRestart: () => Promise<void>
}

export const createLifecycleModule = (ctx: LifecycleContext) => {
  // Listener for Rust-side endpoint state changes. Module-local so siblings
  // can't accidentally overwrite a live OnceListener instance (whose unlisten
  // would then be unreachable, leaving the Tauri-side listener leaked).
  let stateEvents: OnceListener | null = null

  const refreshStatusAsync = async () => {
    try {
      const status = await invoke<PeerStorageStatus>('peer_storage_status')
      ctx.running.value = status.running
      ctx.nodeId.value = status.nodeId
    } catch (error) {
      log.error('Failed to get status:', error)
    }
  }

  const startEndpointAsync = async (deviceStore: ReturnType<typeof useDeviceStore>) => {
    // Make sure the iroh endpoint runs with the device's persistent secret
    // key, not the ephemeral one PeerEndpoint::new_ephemeral created.
    await deviceStore.loadEndpointKeyAsync()

    await ctx.loadConfiguredRelayUrlAsync()
    const info = await invoke<PeerStorageStartInfo>('peer_storage_start', {
      relayUrl: ctx.configuredRelayUrl.value || null,
    })
    ctx.running.value = true
    ctx.nodeId.value = info.nodeId
    ctx.relayUrl.value = info.relayUrl

    await ctx.loadSpaceDevicesAsync()
    await ctx.loadAcceptedInviteEndpointsAsync()
    if (ctx.relayUrl.value) {
      const db = requireDb()
      // Refresh the relay URL on our publish rows so peers see the current
      // one. We match by the random device row id (FK on haex_devices.id),
      // not by endpoint id, because endpoint id changes on reclaim.
      await db
        .update(haexSpaceDevices)
        .set({ relayUrl: ctx.relayUrl.value })
        .where(eq(haexSpaceDevices.deviceId, deviceStore.deviceRowId))
    }

    // Start leader mode for local spaces now that the P2P endpoint is active
    const spacesStore = useSpacesStore()
    await spacesStore.startLocalSpaceLeadersAsync()

    // For spaces where another device is the elected leader, start a peer
    // sync loop so we pull CRDT history.
    await spacesStore.startLocalSpacePeerSyncAsync()

    // Owner-device sync: serverless P2P sync of the owner's OWN vault across the
    // owner's other devices. Default-ON (a missing per-device setting = enabled).
    try {
      const ownerDb = requireDb()
      const row = deviceStore.deviceId
        ? await ownerDb.query.haexVaultSettings.findFirst({
            where: and(
              eq(haexVaultSettings.key, VaultSettingsKeyEnum.ownerSyncAutostart),
              eq(haexVaultSettings.deviceId, deviceStore.deviceId),
            ),
          })
        : undefined
      if (ownerSyncAutostartEnabled(row?.value)) {
        await ctx.startOwnerSyncAsync()
      }
    } catch (error) {
      log.warn('[owner-sync] autostart skipped:', error)
    }

    // Start enabled file sync rules
    const fileSyncStore = useFileSyncStore()
    await fileSyncStore.loadRulesAsync()
    await fileSyncStore.startEnabledRulesAsync()

    // Listen for Rust-side endpoint state changes. When Android suspends the
    // process, iroh closes the endpoint and emits this event. We restart the
    // full P2P stack so the user doesn't have to relaunch the app.
    //
    // The handler re-enters `startAsync()` on close-event; gate creation so
    // we don't overwrite a live OnceListener instance (whose unlisten would
    // then be unreachable, leaving the Tauri-side listener leaked).
    if (!stateEvents) {
      stateEvents = createOnceListener(() =>
        listen<PeerStorageStateEvent>(
          RUST_EVENTS.peerStorageStateChanged,
          (event) => {
            const { running: isRunning, reason, uptimeSecs } = event.payload
            if (!isRunning && ctx.running.value) {
              log.warn(`[P2P] Endpoint closed (reason=${reason}, uptime=${uptimeSecs}s), restarting`)
              ctx.running.value = false
              // The Rust owner-sync loops are torn down with the endpoint; just
              // reset the flag. The startAsync() restart re-runs the autostart
              // path, so don't issue a Rust stop here.
              ctx.ownerSyncRunning.value = false
              ctx.requestRestart().catch(err => log.error('[P2P] Post-close restart failed:', err))
            }
          },
          { target: 'main' },
        ),
      )
    }
    await stateEvents.initAsync()
  }

  const stopAsync = async () => {
    stateEvents?.dispose()
    stateEvents = null

    // Best-effort: a failure here must not block tearing down the endpoint.
    try {
      await ctx.stopOwnerSyncAsync()
    } catch (error) {
      log.warn('[owner-sync] stop failed:', error)
    }

    try {
      await invoke('file_sync_stop_all')
    } catch { /* ok if no syncs running */ }

    await invoke('peer_storage_stop')
    ctx.running.value = false
  }

  return {
    refreshStatusAsync,
    startEndpointAsync,
    stopAsync,
  }
}
