import { eq } from 'drizzle-orm'
import {
  haexPasswordsItemDetails,
  haexPasswordsItemTags,
  haexPasswordsTags,
} from '~/database/schemas'
import type {
  SelectHaexPasswordsItemDetails,
  SelectHaexPasswordsTags,
} from '~/database/schemas'
import { requireDb } from '~/stores/vault'

type ItemTagRow = SelectHaexPasswordsTags & { itemId: string }

export type PasswordsViewMode = 'list' | 'item'

export const usePasswordsStore = defineStore('passwordsStore', () => {
  const items = ref<SelectHaexPasswordsItemDetails[]>([])
  const itemTagRows = ref<ItemTagRow[]>([])
  const selectedItemId = ref<string | null>(null)
  const viewMode = ref<PasswordsViewMode>('list')
  const isEditing = ref(false)

  const loadItemsAsync = async () => {
    const db = requireDb()
    items.value = await db.select().from(haexPasswordsItemDetails)
    itemTagRows.value = await db
      .select({
        itemId: haexPasswordsItemTags.itemId,
        id: haexPasswordsTags.id,
        name: haexPasswordsTags.name,
        color: haexPasswordsTags.color,
        createdAt: haexPasswordsTags.createdAt,
      })
      .from(haexPasswordsItemTags)
      .innerJoin(
        haexPasswordsTags,
        eq(haexPasswordsItemTags.tagId, haexPasswordsTags.id),
      )
  }

  const tagsByItemId = computed<Record<string, SelectHaexPasswordsTags[]>>(
    () => {
      const map: Record<string, SelectHaexPasswordsTags[]> = {}
      for (const row of itemTagRows.value) {
        const { itemId, ...tag } = row
        if (!map[itemId]) map[itemId] = []
        map[itemId].push(tag)
      }
      return map
    },
  )

  const selectedItem = computed(() => {
    if (!selectedItemId.value) return null
    return items.value.find((item) => item.id === selectedItemId.value) ?? null
  })

  const selectedItemTags = computed(() => {
    if (!selectedItemId.value) return []
    return tagsByItemId.value[selectedItemId.value] ?? []
  })

  const itemsInSelectedGroup = computed<SelectHaexPasswordsItemDetails[]>(() => {
    const groupsStore = usePasswordsGroupsStore()
    const activeGroupId = groupsStore.selectedGroupId
    if (activeGroupId === null) return items.value
    const itemGroupMap = groupsStore.itemGroupMap
    return items.value.filter(
      (item) => itemGroupMap.get(item.id) === activeGroupId,
    )
  })

  const getTagsForItemAsync = async (
    itemId: string,
  ): Promise<SelectHaexPasswordsTags[]> => {
    const db = requireDb()
    return await db
      .select({
        id: haexPasswordsTags.id,
        name: haexPasswordsTags.name,
        color: haexPasswordsTags.color,
        createdAt: haexPasswordsTags.createdAt,
      })
      .from(haexPasswordsItemTags)
      .innerJoin(
        haexPasswordsTags,
        eq(haexPasswordsItemTags.tagId, haexPasswordsTags.id),
      )
      .where(eq(haexPasswordsItemTags.itemId, itemId))
  }

  type NavSnapshot = {
    selectedItemId: string | null
    viewMode: PasswordsViewMode
    isEditing: boolean
  }

  const snapshotNavState = (): NavSnapshot => ({
    selectedItemId: selectedItemId.value,
    viewMode: viewMode.value,
    isEditing: isEditing.value,
  })

  const restoreNavState = (s: NavSnapshot) => {
    selectedItemId.value = s.selectedItemId
    viewMode.value = s.viewMode
    isEditing.value = s.isEditing
  }

  const openItem = (itemId: string) => {
    selectedItemId.value = itemId
    viewMode.value = 'item'
    isEditing.value = false
  }

  const backToList = () => {
    viewMode.value = 'list'
    isEditing.value = false
  }

  const startCreate = () => {
    selectedItemId.value = null
    viewMode.value = 'item'
    isEditing.value = true
  }

  const startEdit = () => {
    isEditing.value = true
  }

  const cancelEdit = () => {
    // Creating → go back to list; existing → drop back to read view.
    if (selectedItemId.value === null) {
      backToList()
      return
    }
    isEditing.value = false
  }

  const deleteItemAsync = async (itemId: string) => {
    const db = requireDb()
    await db
      .delete(haexPasswordsItemDetails)
      .where(eq(haexPasswordsItemDetails.id, itemId))
    if (selectedItemId.value === itemId) {
      selectedItemId.value = null
    }
    await loadItemsAsync()
  }

  return {
    items,
    selectedItemId,
    viewMode,
    isEditing,
    tagsByItemId,
    selectedItem,
    selectedItemTags,
    itemsInSelectedGroup,
    loadItemsAsync,
    getTagsForItemAsync,
    openItem,
    backToList,
    startCreate,
    startEdit,
    cancelEdit,
    deleteItemAsync,
    snapshotNavState,
    restoreNavState,
  }
})
