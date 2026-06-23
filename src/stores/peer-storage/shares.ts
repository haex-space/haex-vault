import { invoke } from '@tauri-apps/api/core'
import { and, eq, or } from 'drizzle-orm'
import type { Ref } from 'vue'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'
import {
  haexDevices,
  haexIdentities,
  haexPeerShares,
  haexPendingInvites,
  haexSpaceDevices,
  haexVaultSettings,
  type SelectHaexPeerShares,
  type SelectHaexSpaceDevices,
} from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'

const log = createLogger('PEER_STORAGE')

export interface SharesContext {
  shares: Ref<SelectHaexPeerShares[]>
  spaceDevices: Ref<SelectHaexSpaceDevices[]>
  acceptedInviteEndpoints: Ref<Array<{ spaceId: string, endpointId: string }>>
  configuredRelayUrl: Ref<string | null>
  relayUrl: Ref<string | null>
}

export const createSharesModule = (ctx: SharesContext) => {
  const { shares, spaceDevices, acceptedInviteEndpoints, configuredRelayUrl, relayUrl } = ctx

  const loadConfiguredRelayUrlAsync = async () => {
    const db = requireDb()
    const row = await db.query.haexVaultSettings.findFirst({
      where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl),
    })
    configuredRelayUrl.value = row?.value || null
  }

  const saveConfiguredRelayUrlAsync = async (url: string | null) => {
    const db = requireDb()

    const existing = await db.query.haexVaultSettings.findFirst({
      where: eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl),
    })

    if (existing) {
      if (url) {
        await db.update(haexVaultSettings)
          .set({ value: url })
          .where(eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl))
      } else {
        await db.delete(haexVaultSettings)
          .where(eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageRelayUrl))
      }
    } else if (url) {
      await db.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.peerStorageRelayUrl,
        value: url,
      })
    }
    configuredRelayUrl.value = url
  }

  const loadSharesAsync = async () => {
    const db = requireDb()
    shares.value = await db.select().from(haexPeerShares).all()
  }

  const loadSpaceDevicesAsync = async () => {
    const db = requireDb()
    spaceDevices.value = await db.select().from(haexSpaceDevices).all()
  }

  const loadAcceptedInviteEndpointsAsync = async () => {
    const db = requireDb()
    const rows = await db
      .select({
        spaceId: haexPendingInvites.spaceId,
        spaceEndpoints: haexPendingInvites.spaceEndpoints,
      })
      .from(haexPendingInvites)
      .where(eq(haexPendingInvites.status, 'accepted'))
      .all()
    const tuples: Array<{ spaceId: string, endpointId: string }> = []
    for (const row of rows) {
      if (!row.spaceEndpoints) continue
      try {
        const endpoints = JSON.parse(row.spaceEndpoints) as unknown
        if (!Array.isArray(endpoints)) continue
        for (const endpointId of endpoints) {
          if (typeof endpointId === 'string' && endpointId.length > 0) {
            tuples.push({ spaceId: row.spaceId, endpointId })
          }
        }
      } catch {
        // Malformed JSON — skip this invite, don't fail the whole load.
      }
    }
    acceptedInviteEndpoints.value = tuples
  }

  const addShareAsync = async (spaceId: string, name: string, localPath: string) => {
    const db = requireDb()
    const deviceStore = useDeviceStore()
    if (!deviceStore.deviceRowId || !deviceStore.deviceId) {
      throw new Error('Device identity not resolved — cannot add share')
    }

    // Ensure this device is published in the space before adding the share.
    // Without the haex_space_devices row, peers receive the peer_shares row via
    // CRDT sync but cannot resolve the device — `allowed_peers` stays empty and
    // sub-folder listings fail on the auth check. registerDeviceInSpaceAsync is
    // idempotent (upserts on (space_id, endpoint_id)), so calling it on every
    // share-add is safe.
    await registerDeviceInSpaceAsync(spaceId)

    // Self-attribute the row. SyncPush re-injects authored_by_did from the
    // validated UCAN audience, but SyncPull serves rows raw — so a peer
    // pulling the leader's local row would otherwise see NULL, which also
    // disables the haex_peer_shares_ensure_refs trigger and leaves device_id
    // dangling. See validate.rs:52-87 and 0001_late_spyke.sql:130-146.
    //
    // Hydrate the identity store before reading: in some flows
    // (Tauri-restored sessions, freshly-opened vault) the store hasn't
    // loaded yet, and an unhydrated read returns NULL which would
    // reintroduce the exact failure mode the attribution fix is meant to
    // close. loadIdentitiesAsync is idempotent and cheap on cache hit.
    const identityStore = useIdentityStore()
    if (identityStore.ownIdentities.length === 0) {
      await identityStore.loadIdentitiesAsync()
    }
    const authoredByDid = identityStore.ownIdentities[0]?.did ?? null

    await db.insert(haexPeerShares).values({
      spaceId,
      deviceId: deviceStore.deviceRowId,
      endpointId: deviceStore.deviceId,
      name,
      localPath,
      authoredByDid,
    })

    await loadSharesAsync()
    await invoke('peer_storage_reload_shares')
  }

  const removeShareAsync = async (shareId: string) => {
    const db = requireDb()
    await db.delete(haexPeerShares).where(eq(haexPeerShares.id, shareId))
    await loadSharesAsync()
    await invoke('peer_storage_reload_shares')
  }

  /**
   * Publish this device in a space. Called explicitly from the
   * Space-Publishing dialog or the "Geräte & Spaces" matrix settings page —
   * never automatically.
   */
  const registerDeviceInSpaceAsync = async (
    spaceId: string,
    nameOverride?: string,
    identityIdParam?: string,
  ) => {
    const db = requireDb()
    const deviceStore = useDeviceStore()
    if (!deviceStore.deviceRowId || !deviceStore.deviceId) {
      throw new Error('Device identity not resolved — cannot publish in space')
    }

    // Hydrate the identity store before deriving `identityId` /
    // `authoredByDid` from it — see the matching note in `addShareAsync`.
    // loadIdentitiesAsync is idempotent and cheap on cache hit.
    const identityStore = useIdentityStore()
    if (identityStore.ownIdentities.length === 0) {
      await identityStore.loadIdentitiesAsync()
    }
    let identityId = identityIdParam
    if (!identityId) {
      identityId = identityStore.ownIdentities[0]?.id
    }

    if (identityId) {
      const [identityExists] = await db
        .select({ id: haexIdentities.id })
        .from(haexIdentities)
        .where(eq(haexIdentities.id, identityId))
        .limit(1)
      if (!identityExists) {
        log.warn(`Identity ${identityId.substring(0, 8)}... not in DB yet, registering without identity`)
        identityId = undefined
      }
    }

    // Self-attribute the row so SyncPull peers see the author's DID instead
    // of NULL. SyncPush would re-inject this from the validated UCAN, but
    // pulls serve rows raw. See addShareAsync for the same rationale.
    const authoredByDid = identityId
      ? identityStore.identities.find(i => i.id === identityId)?.did ?? null
      : identityStore.ownIdentities[0]?.did ?? null

    const displayName = nameOverride
      || deviceStore.deviceName
      || deviceStore.hostname
      || `Device ${deviceStore.deviceId.slice(0, 8)}`

    // Carry the avatar from the vault-private haex_devices row over to the
    // per-space replica so other members see the same avatar the owner chose
    // in the Welcome dialog. Without this the haex_space_devices row would
    // be inserted with avatar=NULL and the settings UI (which reads from
    // haex_space_devices) would fall back to a seed-only render, diverging
    // from the avatar the user just confirmed.
    const [ownDeviceRow] = await db
      .select({
        avatar: haexDevices.avatar,
        avatarOptions: haexDevices.avatarOptions,
      })
      .from(haexDevices)
      .where(eq(haexDevices.id, deviceStore.deviceRowId))
      .limit(1)
    const avatar = ownDeviceRow?.avatar ?? null
    const avatarOptions = ownDeviceRow?.avatarOptions ?? null

    // Idempotent publish: a previous membership (e.g. leave → re-invite)
    // leaves the haex_space_devices row behind because self-leave only
    // tears down haex_space_members. Re-publishing would otherwise hit a
    // UNIQUE constraint — the table has two: (space_id, endpoint_id) and
    // (space_id, device_id). After a reclaim the endpoint_id rotates but
    // device_id (= haex_devices.id) stays the same, so we have to look up
    // by either column.
    const existing = await db
      .select({ id: haexSpaceDevices.id })
      .from(haexSpaceDevices)
      .where(and(
        eq(haexSpaceDevices.spaceId, spaceId),
        or(
          eq(haexSpaceDevices.endpointId, deviceStore.deviceId),
          eq(haexSpaceDevices.deviceId, deviceStore.deviceRowId),
        ),
      ))
      .limit(1)

    if (existing[0]) {
      // Refresh endpoint_id alongside the rest: a reclaim leaves the row
      // pointing at the rotated-away public key, which would prevent peers
      // from authorising this device on the new endpoint.
      await db.update(haexSpaceDevices)
        .set({
          identityId: identityId || null,
          deviceId: deviceStore.deviceRowId,
          endpointId: deviceStore.deviceId,
          name: displayName,
          platform: deviceStore.platform,
          relayUrl: relayUrl.value,
          authoredByDid,
          avatar,
          avatarOptions,
        })
        .where(eq(haexSpaceDevices.id, existing[0].id))
    } else {
      await db.insert(haexSpaceDevices).values({
        spaceId,
        identityId: identityId || null,
        deviceId: deviceStore.deviceRowId,
        endpointId: deviceStore.deviceId,
        name: displayName,
        platform: deviceStore.platform,
        relayUrl: relayUrl.value,
        authoredByDid,
        avatar,
        avatarOptions,
      })
    }

    await loadSpaceDevicesAsync()
  }

  const unregisterDeviceFromSpaceAsync = async (rowId: string) => {
    const db = requireDb()
    await db.delete(haexSpaceDevices).where(eq(haexSpaceDevices.id, rowId))
    await loadSpaceDevicesAsync()
  }

  return {
    loadConfiguredRelayUrlAsync,
    saveConfiguredRelayUrlAsync,
    loadSharesAsync,
    loadSpaceDevicesAsync,
    loadAcceptedInviteEndpointsAsync,
    addShareAsync,
    removeShareAsync,
    registerDeviceInSpaceAsync,
    unregisterDeviceFromSpaceAsync,
  }
}
