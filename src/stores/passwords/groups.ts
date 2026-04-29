import { eq } from 'drizzle-orm'
import {
  haexPasswordsGroupItems,
  haexPasswordsGroups,
  haexPasswordsItemDetails,
  haexPasswordsItemKeyValues,
  haexPasswordsItemTags,
} from '~/database/schemas'
import type {
  InsertHaexPasswordsGroups,
  SelectHaexPasswordsGroups,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'
import type { SelectionEntry } from '~/stores/passwords/selection'

export const TRASH_GROUP_ID = 'trash'

export type PasswordGroupDraft = Partial<InsertHaexPasswordsGroups> & {
  name: string
}

export const usePasswordsGroupsStore = defineStore(
  'passwordsGroupsStore',
  () => {
    const groups = ref<SelectHaexPasswordsGroups[]>([])
    const itemGroupMap = ref<Map<string, string | null>>(new Map())
    const selectedGroupId = ref<string | null>(null)

    const loadGroupsAsync = async () => {
      const db = requireDb()
      groups.value = await db.select().from(haexPasswordsGroups)
      const links = await db.select().from(haexPasswordsGroupItems)
      itemGroupMap.value = new Map(
        links.map((link) => [link.itemId, link.groupId ?? null]),
      )
    }

    const childrenByParent = computed<
      Map<string | null, SelectHaexPasswordsGroups[]>
    >(() => {
      const map = new Map<string | null, SelectHaexPasswordsGroups[]>()
      for (const group of groups.value) {
        const key = group.parentId ?? null
        const bucket = map.get(key)
        if (bucket) bucket.push(group)
        else map.set(key, [group])
      }
      for (const bucket of map.values()) {
        bucket.sort((a, b) => {
          const orderDelta = (a.sortOrder ?? 0) - (b.sortOrder ?? 0)
          if (orderDelta !== 0) return orderDelta
          return (a.name ?? '').localeCompare(b.name ?? '')
        })
      }
      return map
    })

    const rootGroups = computed<SelectHaexPasswordsGroups[]>(
      () => childrenByParent.value.get(null) ?? [],
    )

    const groupById = computed<Map<string, SelectHaexPasswordsGroups>>(
      () => new Map(groups.value.map((group) => [group.id, group])),
    )

    const trashGroup = computed<SelectHaexPasswordsGroups | null>(
      () => groups.value.find((g) => g.id === TRASH_GROUP_ID) ?? null,
    )

    // Returns true for the trash group itself and all descendants
    const isGroupInTrash = (groupId: string): boolean => {
      if (groupId === TRASH_GROUP_ID) return true
      let cursor: string | null | undefined = groupById.value.get(groupId)?.parentId
      while (cursor) {
        if (cursor === TRASH_GROUP_ID) return true
        cursor = groupById.value.get(cursor)?.parentId
      }
      return false
    }

    const ensureTrashAsync = async (): Promise<void> => {
      if (trashGroup.value) return
      const db = requireDb()
      try {
        await db.insert(haexPasswordsGroups).values({
          id: TRASH_GROUP_ID,
          name: 'Papierkorb',
          icon: 'i-lucide-trash-2',
          parentId: null,
        })
      } catch {
        // Row may already exist in DB but not yet in reactive cache — reload covers it
      }
      await loadGroupsAsync()
    }

    const itemCountByGroupId = computed<Map<string, number>>(() => {
      const counts = new Map<string, number>()
      for (const groupId of itemGroupMap.value.values()) {
        if (!groupId) continue
        counts.set(groupId, (counts.get(groupId) ?? 0) + 1)
      }
      return counts
    })

    const breadcrumbGroups = computed<SelectHaexPasswordsGroups[]>(() => {
      if (!selectedGroupId.value) return []
      const chain: SelectHaexPasswordsGroups[] = []
      let cursor: string | null | undefined = selectedGroupId.value
      const lookup = groupById.value
      while (cursor) {
        const node = lookup.get(cursor)
        if (!node) break
        chain.unshift(node)
        cursor = node.parentId ?? null
      }
      return chain
    })

    const descendantIdSet = (groupId: string): Set<string> => {
      const result = new Set<string>([groupId])
      const stack: string[] = [groupId]
      while (stack.length > 0) {
        const current = stack.pop()!
        const children = childrenByParent.value.get(current) ?? []
        for (const child of children) {
          if (!result.has(child.id)) {
            result.add(child.id)
            stack.push(child.id)
          }
        }
      }
      return result
    }

    const addGroupAsync = async (draft: PasswordGroupDraft): Promise<string> => {
      const db = requireDb()
      const id = draft.id ?? crypto.randomUUID()
      await db.insert(haexPasswordsGroups).values({
        id,
        name: draft.name,
        description: draft.description ?? null,
        icon: draft.icon ?? null,
        color: draft.color ?? null,
        sortOrder: draft.sortOrder ?? null,
        parentId: draft.parentId ?? null,
      }).onConflictDoNothing()
      await loadGroupsAsync()
      return id
    }

    const updateGroupAsync = async (
      group: SelectHaexPasswordsGroups,
    ): Promise<void> => {
      const db = requireDb()
      await db
        .update(haexPasswordsGroups)
        .set({
          name: group.name,
          description: group.description,
          icon: group.icon,
          color: group.color,
          sortOrder: group.sortOrder,
          parentId: group.parentId,
        })
        .where(eq(haexPasswordsGroups.id, group.id))
      await loadGroupsAsync()
    }

    const deleteGroupAsync = async (groupId: string): Promise<void> => {
      if (groupId === TRASH_GROUP_ID) return
      const db = requireDb()
      if (isGroupInTrash(groupId)) {
        // Already in trash → permanent delete (cascade removes descendants + items)
        await db.delete(haexPasswordsGroups).where(eq(haexPasswordsGroups.id, groupId))
      } else {
        // Move group into trash (preserves folder structure inside trash)
        await ensureTrashAsync()
        await db
          .update(haexPasswordsGroups)
          .set({ parentId: TRASH_GROUP_ID })
          .where(eq(haexPasswordsGroups.id, groupId))
      }
      await loadGroupsAsync()
      if (
        selectedGroupId.value &&
        !groups.value.some((group) => group.id === selectedGroupId.value)
      ) {
        selectedGroupId.value = null
      }
    }

    const moveGroupAsync = async (
      groupId: string,
      targetParentId: string | null,
    ): Promise<void> => {
      const db = requireDb()
      await db
        .update(haexPasswordsGroups)
        .set({ parentId: targetParentId })
        .where(eq(haexPasswordsGroups.id, groupId))
      await loadGroupsAsync()
    }

    const setItemGroupAsync = async (
      itemId: string,
      targetGroupId: string | null,
    ): Promise<void> => {
      const db = requireDb()
      const existing = await db
        .select()
        .from(haexPasswordsGroupItems)
        .where(eq(haexPasswordsGroupItems.itemId, itemId))
        .limit(1)

      if (existing.length > 0) {
        await db
          .update(haexPasswordsGroupItems)
          .set({ groupId: targetGroupId })
          .where(eq(haexPasswordsGroupItems.itemId, itemId))
      } else {
        await db.insert(haexPasswordsGroupItems).values({
          itemId,
          groupId: targetGroupId,
        })
      }
      itemGroupMap.value.set(itemId, targetGroupId)
    }

    const selectGroup = (groupId: string | null) => {
      selectedGroupId.value = groupId
    }

    // Bulk operations ---------------------------------------------------------

    const deleteItemAsync = async (itemId: string): Promise<void> => {
      const db = requireDb()
      const currentGroupId = itemGroupMap.value.get(itemId) ?? null
      if (currentGroupId !== null && isGroupInTrash(currentGroupId)) {
        // Already in trash → permanent delete (FK cascade handles related rows)
        await db
          .delete(haexPasswordsItemDetails)
          .where(eq(haexPasswordsItemDetails.id, itemId))
      } else {
        // Move to trash
        await ensureTrashAsync()
        await setItemGroupAsync(itemId, TRASH_GROUP_ID)
      }
    }

    const bulkDeleteAsync = async (entries: SelectionEntry[]): Promise<void> => {
      for (const entry of entries) {
        if (entry.type === 'item') {
          await deleteItemAsync(entry.id)
        } else {
          await deleteGroupAsync(entry.id)
        }
      }
      await loadGroupsAsync()
    }

    const restoreItemAsync = async (itemId: string): Promise<void> => {
      await setItemGroupAsync(itemId, null)
    }

    const restoreGroupAsync = async (groupId: string): Promise<void> => {
      const db = requireDb()
      await db
        .update(haexPasswordsGroups)
        .set({ parentId: null })
        .where(eq(haexPasswordsGroups.id, groupId))
      await loadGroupsAsync()
    }

    // Move multiple entries to a target group. Throws if any group in the
    // selection would create a cycle (target is a descendant of a moved group).
    const bulkMoveAsync = async (
      entries: SelectionEntry[],
      targetGroupId: string | null,
    ): Promise<void> => {
      if (targetGroupId !== null) {
        for (const entry of entries) {
          if (entry.type !== 'group') continue
          if (entry.id === targetGroupId) {
            throw new Error('selfMove')
          }
          if (descendantIdSet(entry.id).has(targetGroupId)) {
            throw new Error('cycleMove')
          }
        }
      }
      for (const entry of entries) {
        if (entry.type === 'group') {
          await moveGroupAsync(entry.id, targetGroupId)
        } else {
          await setItemGroupAsync(entry.id, targetGroupId)
        }
      }
    }

    // Duplicate an item (details + key_values + tag links) into a target group.
    // Snapshots are NOT copied — they're per-item history, not semantic content.
    const cloneItemAsync = async (
      itemId: string,
      targetGroupId: string | null,
    ): Promise<string | null> => {
      const db = requireDb()
      const rows = await db
        .select()
        .from(haexPasswordsItemDetails)
        .where(eq(haexPasswordsItemDetails.id, itemId))
        .limit(1)
      const source = rows[0]
      if (!source) return null

      const newId = crypto.randomUUID()
      const now = new Date().toISOString()
      await db.insert(haexPasswordsItemDetails).values({
        ...source,
        id: newId,
        createdAt: now,
        updatedAt: now,
      })

      const keyValues = await db
        .select()
        .from(haexPasswordsItemKeyValues)
        .where(eq(haexPasswordsItemKeyValues.itemId, itemId))
      if (keyValues.length) {
        await db.insert(haexPasswordsItemKeyValues).values(
          keyValues.map((kv) => ({
            id: crypto.randomUUID(),
            itemId: newId,
            key: kv.key,
            value: kv.value,
            updatedAt: now,
          })),
        )
      }

      const tagLinks = await db
        .select()
        .from(haexPasswordsItemTags)
        .where(eq(haexPasswordsItemTags.itemId, itemId))
      if (tagLinks.length) {
        await db.insert(haexPasswordsItemTags).values(
          tagLinks.map((link) => ({
            id: crypto.randomUUID(),
            itemId: newId,
            tagId: link.tagId,
          })),
        )
      }

      await db.insert(haexPasswordsGroupItems).values({
        itemId: newId,
        groupId: targetGroupId,
      })
      return newId
    }

    // Recursively clone a group, its child groups, and the items inside each.
    const cloneGroupRecursiveAsync = async (
      groupId: string,
      targetParentId: string | null,
    ): Promise<void> => {
      const source = groups.value.find((group) => group.id === groupId)
      if (!source) return

      const newGroupId = await addGroupAsync({
        name: source.name ?? '',
        description: source.description ?? null,
        icon: source.icon ?? null,
        color: source.color ?? null,
        parentId: targetParentId,
      })

      const childGroupsSnapshot = (childrenByParent.value.get(groupId) ?? []).slice()
      for (const child of childGroupsSnapshot) {
        await cloneGroupRecursiveAsync(child.id, newGroupId)
      }

      const itemIdsInGroup: string[] = []
      for (const [itemId, gid] of itemGroupMap.value.entries()) {
        if (gid === groupId) itemIdsInGroup.push(itemId)
      }
      for (const itemId of itemIdsInGroup) {
        await cloneItemAsync(itemId, newGroupId)
      }
    }

    const bulkCloneAsync = async (
      entries: SelectionEntry[],
      targetGroupId: string | null,
    ): Promise<void> => {
      for (const entry of entries) {
        if (entry.type === 'item') {
          await cloneItemAsync(entry.id, targetGroupId)
        } else {
          await cloneGroupRecursiveAsync(entry.id, targetGroupId)
        }
      }
      await loadGroupsAsync()
    }

    return {
      groups,
      itemGroupMap,
      selectedGroupId,
      rootGroups,
      childrenByParent,
      groupById,
      descendantIdSet,
      itemCountByGroupId,
      breadcrumbGroups,
      trashGroup,
      isGroupInTrash,
      ensureTrashAsync,
      loadGroupsAsync,
      addGroupAsync,
      updateGroupAsync,
      deleteGroupAsync,
      moveGroupAsync,
      setItemGroupAsync,
      selectGroup,
      deleteItemAsync,
      bulkDeleteAsync,
      bulkMoveAsync,
      bulkCloneAsync,
      restoreItemAsync,
      restoreGroupAsync,
    }
  },
)
