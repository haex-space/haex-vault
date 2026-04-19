import { eq } from 'drizzle-orm'
import {
  haexPasswordsGroupItems,
  haexPasswordsGroups,
} from '~/database/schemas'
import type {
  InsertHaexPasswordsGroups,
  SelectHaexPasswordsGroups,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'

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
      })
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
      const db = requireDb()
      await db.delete(haexPasswordsGroups).where(eq(haexPasswordsGroups.id, groupId))
      await loadGroupsAsync()
      // FK-cascade may have tombstoned descendants too; reset selection if the
      // current choice no longer exists.
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
      loadGroupsAsync,
      addGroupAsync,
      updateGroupAsync,
      deleteGroupAsync,
      moveGroupAsync,
      setItemGroupAsync,
      selectGroup,
    }
  },
)
