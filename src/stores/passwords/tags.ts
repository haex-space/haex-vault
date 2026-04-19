import { eq } from 'drizzle-orm'
import {
  haexPasswordsItemTags,
  haexPasswordsTags,
} from '~/database/schemas'
import type { SelectHaexPasswordsTags } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

export const usePasswordsTagsStore = defineStore('passwordsTagsStore', () => {
  const tags = ref<SelectHaexPasswordsTags[]>([])

  const loadTagsAsync = async () => {
    const db = requireDb()
    tags.value = await db
      .select()
      .from(haexPasswordsTags)
      .orderBy(haexPasswordsTags.name)
  }

  // Case-insensitive name lookup; returns existing tag or inserts a new one.
  const getOrCreateTagAsync = async (
    name: string,
    color: string | null = null,
  ): Promise<SelectHaexPasswordsTags> => {
    const trimmed = name.trim()
    if (!trimmed) throw new Error('Tag name cannot be empty')

    const normalized = trimmed.toLowerCase()
    const existing = tags.value.find(
      (t) => t.name.toLowerCase() === normalized,
    )
    if (existing) return existing

    const db = requireDb()
    const newTag: SelectHaexPasswordsTags = {
      id: crypto.randomUUID(),
      name: trimmed,
      color,
      createdAt: new Date().toISOString(),
    }
    await db.insert(haexPasswordsTags).values(newTag)
    tags.value.push(newTag)
    return newTag
  }

  // Diff-based item-tag sync — CRDT-friendly: only touches rows that actually change.
  const setItemTagsAsync = async (itemId: string, nextTagIds: string[]) => {
    const db = requireDb()
    const currentLinks = await db
      .select()
      .from(haexPasswordsItemTags)
      .where(eq(haexPasswordsItemTags.itemId, itemId))

    const currentTagIds = new Set(currentLinks.map((l) => l.tagId))
    const nextSet = new Set(nextTagIds)

    for (const link of currentLinks) {
      if (!nextSet.has(link.tagId)) {
        await db
          .delete(haexPasswordsItemTags)
          .where(eq(haexPasswordsItemTags.id, link.id))
      }
    }

    for (const tagId of nextTagIds) {
      if (!currentTagIds.has(tagId)) {
        await db.insert(haexPasswordsItemTags).values({
          id: crypto.randomUUID(),
          itemId,
          tagId,
        })
      }
    }
  }

  // Resolve a list of tag names (case-insensitive) into tag records, creating new ones on the fly.
  const resolveTagNamesAsync = async (
    names: string[],
  ): Promise<SelectHaexPasswordsTags[]> => {
    const resolved: SelectHaexPasswordsTags[] = []
    for (const name of names) {
      if (!name.trim()) continue
      resolved.push(await getOrCreateTagAsync(name))
    }
    return resolved
  }

  return {
    tags,
    loadTagsAsync,
    getOrCreateTagAsync,
    resolveTagNamesAsync,
    setItemTagsAsync,
  }
})
