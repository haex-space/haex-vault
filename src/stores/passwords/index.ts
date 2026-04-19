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

export const usePasswordsStore = defineStore('passwordsStore', () => {
  const items = ref<SelectHaexPasswordsItemDetails[]>([])
  const itemTagRows = ref<ItemTagRow[]>([])
  const selectedItemId = ref<string | null>(null)

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

  // Grouped view of the flat itemTagRows, indexed by item id.
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

  const selectItem = (itemId: string | null) => {
    selectedItemId.value = itemId
  }

  return {
    items,
    selectedItemId,
    tagsByItemId,
    loadItemsAsync,
    getTagsForItemAsync,
    selectItem,
  }
})
