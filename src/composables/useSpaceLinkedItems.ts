import { and, eq } from 'drizzle-orm'
import { haexSharedSpaceSync, type SelectHaexSharedSpaceSync } from '~/database/schemas'

export interface SpaceLinkedItemGroup {
  /** Unique key for the group (typically the extensionId) */
  key: string
  /** Display label for the group */
  label: string
  /** Icon for the group */
  icon: string
  /** Type discriminator — currently only 'extension', reserved for future types */
  type: 'extension'
  /** Extension ID (only for type 'extension') */
  extensionId?: string
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
  const { getDb } = useVaultDb()
  const extensionsStore = useExtensionsStore()

  const isLoading = ref(false)
  const spaceAssignments = ref<SelectHaexSharedSpaceSync[]>([])

  const loadAsync = async () => {
    const db = getDb()
    if (!db) return

    isLoading.value = true
    try {
      const id = toValue(spaceId)

      spaceAssignments.value = await db
        .select()
        .from(haexSharedSpaceSync)
        .where(eq(haexSharedSpaceSync.spaceId, id))
    } finally {
      isLoading.value = false
    }
  }

  /** Resolve a portable (publicKey, name) pair to its installed extension */
  const findExtension = (publicKey: string | null, name: string | null) => {
    if (!publicKey || !name) return null
    return extensionsStore.availableExtensions.find(
      (ext) => ext.publicKey === publicKey && ext.name === name,
    ) ?? null
  }

  /** Remove one displayed extension/category group, or a single assignment by id */
  const removeAssignmentAsync = async (assignment: SelectHaexSharedSpaceSync) => {
    const db = getDb()
    if (!db) return

    if (assignment.category) {
      await db.delete(haexSharedSpaceSync).where(
        and(
          eq(haexSharedSpaceSync.category, assignment.category),
          eq(haexSharedSpaceSync.spaceId, assignment.spaceId),
          assignment.extensionPublicKey && assignment.extensionName
            ? and(
                eq(haexSharedSpaceSync.extensionPublicKey, assignment.extensionPublicKey),
                eq(haexSharedSpaceSync.extensionName, assignment.extensionName),
              )
            : eq(haexSharedSpaceSync.tableName, assignment.tableName),
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

    // Extension data — group by (extensionPublicKey, extensionName, category) for logical units
    const byExtension = new Map<string, {
      ext: ReturnType<typeof findExtension>
      /** Logical groups within this extension (by category) */
      logicalGroups: Map<string, SelectHaexSharedSpaceSync[]>
    }>()

    for (const assignment of spaceAssignments.value) {
      const extKey = assignment.extensionPublicKey && assignment.extensionName
        ? `${assignment.extensionPublicKey}__${assignment.extensionName}`
        : assignment.tableName
      if (!byExtension.has(extKey)) {
        byExtension.set(extKey, {
          ext: findExtension(assignment.extensionPublicKey, assignment.extensionName),
          logicalGroups: new Map(),
        })
      }
      // Group by category within extension; ungrouped items get their own id as key
      const groupKey = assignment.category ?? `_ungrouped_${assignment.id}`
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
        // Use metadata from the first assignment that has typeLabel/type
        const representative = assignments.find((a) => a.typeLabel) ?? assignments[0]
        if (!representative) continue

        items.push({
          label: representative.typeLabel || representative.tableName.split('__').pop() || representative.tableName,
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
          extensionId: ext?.id,
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
