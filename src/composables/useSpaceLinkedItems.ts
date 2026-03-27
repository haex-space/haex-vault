import { and, eq } from 'drizzle-orm'
import { haexPeerShares, haexSharedSpaceSync, type SelectHaexPeerShares, type SelectHaexSharedSpaceSync } from '~/database/schemas'

export interface SpaceLinkedItemGroup {
  /** Unique key for the group (e.g. 'p2p-shares' or extensionId) */
  key: string
  /** Display label for the group */
  label: string
  /** Icon for the group */
  icon: string
  /** Type discriminator */
  type: 'p2p-shares' | 'extension'
  /** Individual items within this group */
  items: SpaceLinkedItem[]
}

export interface SpaceLinkedItem {
  /** Display label */
  label: string
  /** Optional subtitle (e.g. type category or file path) */
  subtitle?: string
  /** Icon for the item */
  icon: string
  /** Callback to remove this item from the space */
  remove: () => Promise<void>
}

export function useSpaceLinkedItems(spaceId: MaybeRefOrGetter<string>) {
  const { currentVault } = storeToRefs(useVaultStore())
  const peerStorageStore = usePeerStorageStore()
  const extensionsStore = useExtensionsStore()

  const isLoading = ref(false)
  const peerShares = ref<SelectHaexPeerShares[]>([])
  const spaceAssignments = ref<SelectHaexSharedSpaceSync[]>([])

  const loadAsync = async () => {
    const db = currentVault.value?.drizzle
    if (!db) return

    isLoading.value = true
    try {
      const id = toValue(spaceId)

      const [shares, assignments] = await Promise.all([
        db.select().from(haexPeerShares).where(eq(haexPeerShares.spaceId, id)),
        db.select().from(haexSharedSpaceSync).where(eq(haexSharedSpaceSync.spaceId, id)),
      ])

      peerShares.value = shares
      spaceAssignments.value = assignments
    } finally {
      isLoading.value = false
    }
  }

  /** Resolve an extensionId to its installed extension */
  const findExtension = (extensionId: string | null) => {
    if (!extensionId) return null
    return extensionsStore.availableExtensions.find((ext) => ext.id === extensionId) ?? null
  }

  /** Remove all assignments sharing the same groupId+spaceId, or a single assignment by id */
  const removeAssignmentAsync = async (assignment: SelectHaexSharedSpaceSync) => {
    const db = currentVault.value?.drizzle
    if (!db) return

    if (assignment.groupId) {
      await db.delete(haexSharedSpaceSync).where(
        and(
          eq(haexSharedSpaceSync.groupId, assignment.groupId),
          eq(haexSharedSpaceSync.spaceId, assignment.spaceId),
        ),
      )
    } else {
      await db.delete(haexSharedSpaceSync).where(
        eq(haexSharedSpaceSync.id, assignment.id),
      )
    }
    await loadAsync()
  }

  /** Build grouped linked items from raw data */
  const groups = computed<SpaceLinkedItemGroup[]>(() => {
    const result: SpaceLinkedItemGroup[] = []

    // P2P Shares
    if (peerShares.value.length > 0) {
      result.push({
        key: 'p2p-shares',
        label: 'P2P Storage',
        icon: 'i-lucide-hard-drive',
        type: 'p2p-shares',
        items: peerShares.value.map((share) => ({
          label: share.name,
          subtitle: share.localPath,
          icon: share.name.includes('.') && !share.name.endsWith('/')
            ? 'i-lucide-file'
            : 'i-lucide-folder',
          remove: async () => {
            await peerStorageStore.removeShareAsync(share.id)
            await loadAsync()
          },
        })),
      })
    }

    // Extension data — group by (extensionId, groupId) for logical units
    const byExtension = new Map<string, {
      ext: ReturnType<typeof findExtension>
      /** Logical groups within this extension (by groupId) */
      logicalGroups: Map<string, SelectHaexSharedSpaceSync[]>
    }>()

    for (const assignment of spaceAssignments.value) {
      const extKey = assignment.extensionId ?? assignment.tableName
      if (!byExtension.has(extKey)) {
        byExtension.set(extKey, {
          ext: findExtension(assignment.extensionId),
          logicalGroups: new Map(),
        })
      }
      // Group by groupId within extension; ungrouped items get their own id as key
      const groupKey = assignment.groupId ?? `_ungrouped_${assignment.id}`
      const entry = byExtension.get(extKey)!
      if (!entry.logicalGroups.has(groupKey)) {
        entry.logicalGroups.set(groupKey, [])
      }
      entry.logicalGroups.get(groupKey)!.push(assignment)
    }

    for (const [extKey, { ext, logicalGroups }] of byExtension) {
      const extensionIcon = ext?.iconUrl ?? ext?.icon ?? 'i-heroicons-puzzle-piece-solid'

      // Each logical group becomes one item in the extension group
      const items: SpaceLinkedItem[] = []
      for (const [, assignments] of logicalGroups) {
        // Use metadata from the first assignment that has label/type
        const representative = assignments.find((a) => a.label) ?? assignments[0]

        items.push({
          label: representative.label || representative.tableName.split('__').pop() || representative.tableName,
          subtitle: representative.type ?? undefined,
          icon: extensionIcon,
          remove: async () => removeAssignmentAsync(representative),
        })
      }

      if (items.length > 0) {
        result.push({
          key: extKey,
          label: ext?.name ?? extKey,
          icon: extensionIcon,
          type: 'extension',
          items,
        })
      }
    }

    return result
  })

  const totalCount = computed(() => groups.value.reduce((sum, g) => sum + g.items.length, 0))

  return {
    isLoading,
    groups,
    totalCount,
    loadAsync,
  }
}
