import { invoke } from '@tauri-apps/api/core'
import type { RemotePeer } from '~/composables/fileBrowserHelpers'
import type { StorageBackendInfo } from '~/../src-tauri/bindings/StorageBackendInfo'

export type GroupBy = 'space' | 'contact'

export interface AvatarRef {
  src?: string | null
  seed?: string
  options?: Record<string, unknown> | null
  alt?: string
}

export interface OverviewEntry {
  kind: 'local-share' | 'remote-peer' | 'cloud-backend'
  key: string
  title: string
  subtitle: string
  icon?: string
  avatar?: AvatarRef
  badge?: AvatarRef
  peer: RemotePeer
}

export interface OverviewGroup {
  id: string
  title: string
  subtitle?: string
  icon?: string
  avatar?: AvatarRef
  entries: OverviewEntry[]
}

export interface PeerEntryInput {
  endpointId: string
  contextKey: string
  detail: string
  source: RemotePeer['source']
  // Optional rich data (preferred when available)
  device?: ReturnType<typeof usePeerStorageStore>['spaceDevices'][number]
  identityId?: string | null
  // Fallback name when neither identity nor device row are available
  fallbackName?: string
}

/**
 * Build the storage-overview groups shown when no peer is selected in the
 * file browser. Aggregates: local shares, remote space devices, direct
 * contact-claim devices, and S3 cloud backends — then groups them by space
 * or by contact depending on `groupBy.value`.
 *
 * Extracted from `components/haex/system/files/index.vue` (was 1297-1822 in
 * the original monolith). Pure structural extraction; ZERO logic changes.
 */
export function useFilesOverviewGroups() {
  const { t } = useI18n()
  const peerStore = usePeerStorageStore()
  const spacesStore = useSpacesStore()
  const identityStore = useIdentityStore()

  const groupBy = ref<GroupBy>('space')

  /**
   * Identifies whether a given endpoint id belongs to this device. Returns
   * true when `peerStore.nodeId` is empty so we never expose an own
   * `haex_space_devices` row as a remote peer during the brief window
   * before `refreshStatusAsync` resolves (or after `stopAsync`, which
   * resets `nodeId` to '' even though the row in DB is still ours).
   *
   * Biases toward "treat unknown endpoints as own" because the alternative
   * — surfacing the local device as a peer — is the more confusing failure
   * mode. Once `nodeId` is populated this collapses to the strict equality
   * check.
   */
  const isOwnEndpoint = (endpointId: string): boolean => {
    if (!peerStore.nodeId) return true
    return endpointId === peerStore.nodeId
  }

  // Aggregate remote peers from spaces + contacts
  const contactClaims = ref<Record<string, { type: string; value: string }[]>>({})
  const loadContactClaimsAsync = async () => {
    for (const contact of identityStore.contacts) {
      const claims = await identityStore.getClaimsAsync(contact.id)
      contactClaims.value[contact.id] = claims.map((c) => ({
        type: c.type,
        value: c.value,
      }))
    }
  }

  // When endpoint is running, filter by nodeId. Otherwise show all shares
  // (they were all registered by this device since they have local paths).
  const localShares = computed(() => {
    if (peerStore.nodeId) {
      return peerStore.shares.filter(
        (s) => s.endpointId === peerStore.nodeId,
      )
    }
    return peerStore.shares
  })

  const getSpaceName = (spaceId: string) => {
    return (
      spacesStore.visibleSpaces.find((s) => s.id === spaceId)?.name ||
      spaceId.slice(0, 8)
    )
  }

  const remotePeers = computed(() => {
    const peers: RemotePeer[] = []
    const seen = new Set<string>()

    for (const device of peerStore.spaceDevices) {
      if (isOwnEndpoint(device.endpointId)) continue
      if (seen.has(device.endpointId)) continue
      seen.add(device.endpointId)
      peers.push({
        endpointId: device.endpointId,
        name: device.name || device.endpointId.slice(0, 16) + '...',
        source: 'space',
        detail: getSpaceName(device.spaceId),
      })
    }

    for (const contact of identityStore.contacts) {
      const claims = contactClaims.value[contact.id] || []
      for (const claim of claims) {
        if (!claim.type.startsWith('device:') || !claim.value) continue
        if (seen.has(claim.value)) continue
        seen.add(claim.value)
        peers.push({
          endpointId: claim.value,
          name: `${contact.name} (${claim.type.replace('device:', '')})`,
          source: 'contact',
          detail: contact.name,
        })
      }
    }

    return peers
  })

  const remotePeerIds = computed(() => remotePeers.value.map((p) => p.endpointId))

  const parseAvatarOptions = (raw: string | null | undefined) => {
    if (!raw) return null
    try {
      return JSON.parse(raw) as Record<string, unknown>
    } catch {
      return null
    }
  }

  const getIdentity = (identityId: string | null | undefined) => {
    if (!identityId) return undefined
    return identityStore.identities.find((i) => i.id === identityId)
  }

  const identityAvatarFromIdentity = (
    identity: ReturnType<typeof getIdentity>,
  ): AvatarRef | undefined => {
    if (!identity) return undefined
    return {
      src: identity.avatar,
      seed: identity.id,
      options: parseAvatarOptions(identity.avatarOptions),
      alt: identity.name,
    }
  }

  const localShareEntry = (
    share: typeof localShares.value[number],
  ): OverviewEntry => ({
    kind: 'local-share',
    key: `local:${share.id}`,
    title: share.name,
    subtitle: t('sections.thisDevice'),
    icon: 'i-lucide-folder',
    peer: {
      endpointId: peerStore.nodeId,
      name: share.name,
      source: 'space',
      detail: t('sections.thisDevice'),
      localPath: share.localPath,
    },
  })

  const buildPeerEntry = (input: PeerEntryInput): OverviewEntry => {
    const identity = getIdentity(input.identityId ?? input.device?.identityId)
    const contactName = identity?.name?.trim() || undefined
    const deviceName =
      input.device?.name?.trim() ||
      input.fallbackName?.trim() ||
      `${input.endpointId.slice(0, 16)}…`

    // Title prefers the contact's known identity name. Subtitle keeps the
    // device name visible when it differs, plus the existing detail line
    // (typically the space name).
    const title = contactName || deviceName
    const showDeviceInSubtitle =
      !!contactName && contactName.toLowerCase() !== deviceName.toLowerCase()
    const subtitle = showDeviceInSubtitle
      ? `${deviceName} · ${input.detail}`
      : input.detail

    const avatar: AvatarRef | undefined = input.device
      ? {
          src: input.device.avatar,
          seed: input.device.endpointId,
          options: parseAvatarOptions(input.device.avatarOptions),
          alt: deviceName,
        }
      : identity
        ? identityAvatarFromIdentity(identity)
        : { seed: input.endpointId, alt: deviceName }

    // Badge is the contact's identity avatar — only shown when we actually
    // have a known identity to badge with AND we already render a separate
    // device avatar (otherwise the identity avatar is the main avatar).
    const badge: AvatarRef | undefined =
      input.device && identity ? identityAvatarFromIdentity(identity) : undefined

    return {
      kind: 'remote-peer',
      key: `remote:${input.contextKey}:${input.endpointId}`,
      title,
      subtitle,
      icon: input.source === 'contact' ? 'i-lucide-user' : 'i-lucide-monitor',
      avatar,
      badge,
      peer: {
        endpointId: input.endpointId,
        name: title,
        source: input.source,
        detail: input.detail,
      },
    }
  }

  // S3 / remote storage backends. These live outside the space + contact model
  // (they belong to no peer), so they get their own group that is always
  // appended last regardless of the current grouping mode.
  const storageBackends = ref<StorageBackendInfo[]>([])
  const loadStorageBackendsAsync = async () => {
    try {
      storageBackends.value = await invoke<StorageBackendInfo[]>(
        'remote_storage_list_backends',
      )
    } catch {
      // Non-fatal: the file browser must still render the other sections even
      // if S3 listing fails (e.g. database error). The settings page is the
      // canonical place to diagnose backend configuration issues.
      storageBackends.value = []
    }
  }

  const s3PeerForBackend = (backend: StorageBackendInfo): RemotePeer => ({
    endpointId: `s3:${backend.id}`,
    name: backend.name,
    source: 's3',
    detail: backend.config?.bucket || backend.type,
    s3BackendId: backend.id,
  })

  const cloudStorageGroup = computed<OverviewGroup | null>(() => {
    const enabled = storageBackends.value.filter((b) => b.enabled)
    if (enabled.length === 0) return null
    return {
      id: 'cloud-storage',
      title: t('groups.cloudStorage'),
      icon: 'i-lucide-cloud',
      entries: enabled.map((backend) => ({
        kind: 'cloud-backend' as const,
        key: `s3:${backend.id}`,
        title: backend.name,
        subtitle: backend.config?.bucket
          ? `${backend.type.toUpperCase()} · ${backend.config.bucket}`
          : backend.type.toUpperCase(),
        icon: 'i-lucide-cloud',
        peer: s3PeerForBackend(backend),
      })),
    }
  })

  // Phantom-row guard: `peerStore.spaceDevices` mirrors every haex_space_devices
  // row in the local DB, including ones that arrived via CRDT sync of a space
  // the user never joined. The spaces store already filters those at the
  // `visibleSpaces` boundary (membership cross-check + owner fallback), so any
  // device whose spaceId is outside that set must not surface in the UI.
  const visibleSpaceIds = computed(
    () => new Set(spacesStore.visibleSpaces.map((s) => s.id)),
  )
  const isDeviceInVisibleSpace = (spaceId: string): boolean =>
    visibleSpaceIds.value.has(spaceId)

  function buildSpaceGroups(): OverviewGroup[] {
    // Bucket entries strictly by spaceId. Two spaces with the same name but
    // different ids stay as two separate groups — they are different spaces
    // by identity and must not be merged. The shortened spaceId is shown as
    // subtitle so the user can tell them apart.
    const buckets = new Map<string, OverviewEntry[]>()
    const seenDevicesPerSpace = new Map<string, Set<string>>()
    const seenSharesPerSpace = new Map<string, Set<string>>()

    const pushEntry = (spaceId: string, entry: OverviewEntry) => {
      const list = buckets.get(spaceId)
      if (list) list.push(entry)
      else buckets.set(spaceId, [entry])
    }

    for (const share of localShares.value) {
      let seen = seenSharesPerSpace.get(share.spaceId)
      if (!seen) {
        seen = new Set()
        seenSharesPerSpace.set(share.spaceId, seen)
      }
      if (seen.has(share.id)) continue
      seen.add(share.id)
      pushEntry(share.spaceId, localShareEntry(share))
    }

    for (const device of peerStore.spaceDevices) {
      if (isOwnEndpoint(device.endpointId)) continue
      if (!isDeviceInVisibleSpace(device.spaceId)) continue
      let seen = seenDevicesPerSpace.get(device.spaceId)
      if (!seen) {
        seen = new Set()
        seenDevicesPerSpace.set(device.spaceId, seen)
      }
      if (seen.has(device.endpointId)) continue
      seen.add(device.endpointId)
      pushEntry(
        device.spaceId,
        buildPeerEntry({
          endpointId: device.endpointId,
          contextKey: `space:${device.spaceId}`,
          detail: getSpaceName(device.spaceId),
          source: 'space',
          device,
        }),
      )
    }

    const groups: OverviewGroup[] = []
    const consumedSpaceIds = new Set<string>()

    const groupForSpace = (
      spaceId: string,
      title: string,
      ownerIdentityId?: string | null,
    ): OverviewGroup => {
      const ownerIdentity = getIdentity(ownerIdentityId)
      return {
        id: `space:${spaceId}`,
        title,
        subtitle: shortSpaceId(spaceId),
        icon: 'i-lucide-layers',
        avatar: identityAvatarFromIdentity(ownerIdentity),
        entries: buckets.get(spaceId) ?? [],
      }
    }

    for (const space of spacesStore.visibleSpaces) {
      if (consumedSpaceIds.has(space.id)) continue
      consumedSpaceIds.add(space.id)
      const entries = buckets.get(space.id)
      if (!entries || entries.length === 0) continue
      groups.push(groupForSpace(space.id, space.name, space.ownerIdentityId))
    }

    // No orphan-spaceId fallback by design: if a bucket's spaceId is not in
    // `visibleSpaces`, the user is not a member and we must not surface that
    // space in the UI — the phantom row got dropped at the bucket-fill step.

    // Direct contact devices (claim-only, not in any space).
    // `peerStore.spaceDevices` is pre-filtered to visible spaces above, so
    // this Set already excludes phantom rows that could otherwise shadow a
    // contact claim sharing the same endpoint.
    const knownEndpointIds = new Set(
      peerStore.spaceDevices
        .filter((d) => isDeviceInVisibleSpace(d.spaceId))
        .map((d) => d.endpointId),
    )
    const directEntries: OverviewEntry[] = []
    const seen = new Set<string>()
    for (const contact of identityStore.contacts) {
      const claims = contactClaims.value[contact.id] || []
      for (const claim of claims) {
        if (!claim.type.startsWith('device:') || !claim.value) continue
        if (knownEndpointIds.has(claim.value)) continue
        if (seen.has(claim.value)) continue
        seen.add(claim.value)
        directEntries.push(
          buildPeerEntry({
            endpointId: claim.value,
            contextKey: 'direct-contacts',
            detail: contact.name,
            source: 'contact',
            identityId: contact.id,
            fallbackName: claim.type.replace('device:', ''),
          }),
        )
      }
    }
    if (directEntries.length > 0) {
      groups.push({
        id: 'direct-contacts',
        title: t('groups.directContacts'),
        icon: 'i-lucide-user',
        entries: directEntries,
      })
    }

    return groups
  }

  function buildContactGroups(): OverviewGroup[] {
    const groups: OverviewGroup[] = []
    const ownIdentityIds = new Set(
      identityStore.ownIdentities.map((i) => i.id),
    )

    // "My devices" — local shares + space devices linked to own identities
    const myEntries: OverviewEntry[] = []
    for (const share of localShares.value) {
      myEntries.push(localShareEntry(share))
    }
    const seenForMe = new Set<string>()
    for (const device of peerStore.spaceDevices) {
      if (isOwnEndpoint(device.endpointId)) continue
      if (seenForMe.has(device.endpointId)) continue
      if (!device.identityId || !ownIdentityIds.has(device.identityId)) continue
      if (!isDeviceInVisibleSpace(device.spaceId)) continue
      seenForMe.add(device.endpointId)
      myEntries.push(
        buildPeerEntry({
          endpointId: device.endpointId,
          contextKey: 'me',
          detail: getSpaceName(device.spaceId),
          source: 'space',
          device,
        }),
      )
    }
    if (myEntries.length > 0) {
      const ownIdentity = identityStore.ownIdentities[0]
      groups.push({
        id: 'me',
        title: t('groups.myDevices'),
        subtitle: ownIdentity?.did ? shortDid(ownIdentity.did) : undefined,
        icon: 'i-lucide-user-check',
        avatar: identityAvatarFromIdentity(ownIdentity),
        entries: myEntries,
      })
    }

    // One group per contact
    for (const contact of identityStore.contacts) {
      const entries: OverviewEntry[] = []
      const seen = new Set<string>()

      for (const device of peerStore.spaceDevices) {
        if (isOwnEndpoint(device.endpointId)) continue
        if (device.identityId !== contact.id) continue
        if (!isDeviceInVisibleSpace(device.spaceId)) continue
        if (seen.has(device.endpointId)) continue
        seen.add(device.endpointId)
        entries.push(
          buildPeerEntry({
            endpointId: device.endpointId,
            contextKey: `contact:${contact.id}`,
            detail: getSpaceName(device.spaceId),
            source: 'space',
            device,
          }),
        )
      }

      const claims = contactClaims.value[contact.id] || []
      for (const claim of claims) {
        if (!claim.type.startsWith('device:') || !claim.value) continue
        if (seen.has(claim.value)) continue
        seen.add(claim.value)
        entries.push(
          buildPeerEntry({
            endpointId: claim.value,
            contextKey: `contact:${contact.id}`,
            detail: contact.name,
            source: 'contact',
            identityId: contact.id,
            fallbackName: claim.type.replace('device:', ''),
          }),
        )
      }

      if (entries.length > 0) {
        groups.push({
          id: `contact:${contact.id}`,
          title: contact.name,
          subtitle: shortDid(contact.did),
          icon: 'i-lucide-user',
          avatar: identityAvatarFromIdentity(contact),
          entries,
        })
      }
    }

    // Devices we know about but cannot attribute to any identity
    const attributedEndpoints = new Set<string>()
    for (const g of groups) {
      for (const e of g.entries) attributedEndpoints.add(e.peer.endpointId)
    }
    const unattributed: OverviewEntry[] = []
    const seenUnattr = new Set<string>()
    for (const device of peerStore.spaceDevices) {
      if (isOwnEndpoint(device.endpointId)) continue
      if (attributedEndpoints.has(device.endpointId)) continue
      if (!isDeviceInVisibleSpace(device.spaceId)) continue
      if (seenUnattr.has(device.endpointId)) continue
      seenUnattr.add(device.endpointId)
      unattributed.push(
        buildPeerEntry({
          endpointId: device.endpointId,
          contextKey: 'unknown',
          detail: getSpaceName(device.spaceId),
          source: 'space',
          device,
        }),
      )
    }
    if (unattributed.length > 0) {
      groups.push({
        id: 'unknown',
        title: t('groups.unknown'),
        icon: 'i-lucide-help-circle',
        entries: unattributed,
      })
    }

    return groups
  }

  function shortDid(did: string): string {
    if (did.length <= 24) return did
    return `${did.slice(0, 16)}…${did.slice(-6)}`
  }

  function shortSpaceId(id: string): string {
    if (id.length <= 12) return id
    return `${id.slice(0, 8)}…${id.slice(-4)}`
  }

  const overviewGroups = computed<OverviewGroup[]>(() => {
    const groups =
      groupBy.value === 'space' ? buildSpaceGroups() : buildContactGroups()
    const cloud = cloudStorageGroup.value
    return cloud ? [...groups, cloud] : groups
  })

  const hasAnyEntries = computed(() => overviewGroups.value.length > 0)

  const loadAsync = async (): Promise<void> => {
    await Promise.all([loadStorageBackendsAsync(), loadContactClaimsAsync()])
  }

  return {
    groupBy,
    overviewGroups,
    hasAnyEntries,
    remotePeers,
    remotePeerIds,
    isOwnEndpoint,
    loadAsync,
  } as const
}
