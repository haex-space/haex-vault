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

export const usePasswordsStore = defineStore('passwordsStore', () => {
  const items = ref<SelectHaexPasswordsItemDetails[]>([])

  const loadItemsAsync = async () => {
    const db = requireDb()
    items.value = await db.select().from(haexPasswordsItemDetails)
  }

  const getTagsForItemAsync = async (
    itemId: string,
  ): Promise<SelectHaexPasswordsTags[]> => {
    const db = requireDb()
    const rows = await db
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
    return rows
  }

  return {
    items,
    loadItemsAsync,
    getTagsForItemAsync,
  }
})
