import { eq, sql } from 'drizzle-orm'
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
    await db.insert(haexPasswordsTags).values(newTag).onConflictDoNothing()
    const rows = await db.select().from(haexPasswordsTags).where(eq(haexPasswordsTags.name, trimmed)).limit(1)
    const tag = rows[0] ?? newTag
    if (!tags.value.find((t) => t.id === tag.id)) tags.value.push(tag)
    return tag
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

  // Per-tag item counts. Returns a map: tagId -> number of items using it.
  const getItemCountsAsync = async (): Promise<Map<string, number>> => {
    const db = requireDb()
    const rows = await db
      .select({
        tagId: haexPasswordsItemTags.tagId,
        count: sql<number>`count(*)`,
      })
      .from(haexPasswordsItemTags)
      .groupBy(haexPasswordsItemTags.tagId)
    return new Map(rows.map((row) => [row.tagId, Number(row.count)]))
  }

  const renameAsync = async (id: string, name: string): Promise<void> => {
    const trimmed = name.trim()
    if (!trimmed) throw new Error('Tag name cannot be empty')
    const db = requireDb()
    await db
      .update(haexPasswordsTags)
      .set({ name: trimmed })
      .where(eq(haexPasswordsTags.id, id))
    const local = tags.value.find((t) => t.id === id)
    if (local) local.name = trimmed
  }

  const updateColorAsync = async (
    id: string,
    color: string | null,
  ): Promise<void> => {
    const db = requireDb()
    await db
      .update(haexPasswordsTags)
      .set({ color })
      .where(eq(haexPasswordsTags.id, id))
    const local = tags.value.find((t) => t.id === id)
    if (local) local.color = color
  }

  // Cascade on tagId drops item-tag rows automatically.
  const deleteAsync = async (id: string): Promise<void> => {
    const db = requireDb()
    await db
      .delete(haexPasswordsTags)
      .where(eq(haexPasswordsTags.id, id))
    tags.value = tags.value.filter((t) => t.id !== id)
  }

  // Insert item-tag links, skipping items that already have the tag.
  const bulkAddTagAsync = async (
    itemIds: string[],
    tagId: string,
  ): Promise<number> => {
    const db = requireDb()
    const existing = await db
      .select({ itemId: haexPasswordsItemTags.itemId })
      .from(haexPasswordsItemTags)
      .where(eq(haexPasswordsItemTags.tagId, tagId))
    const alreadyTagged = new Set(existing.map((row) => row.itemId))

    let added = 0
    for (const itemId of itemIds) {
      if (alreadyTagged.has(itemId)) continue
      await db.insert(haexPasswordsItemTags).values({
        id: crypto.randomUUID(),
        itemId,
        tagId,
      })
      added++
    }
    return added
  }

  const bulkRemoveTagAsync = async (
    itemIds: string[],
    tagId: string,
  ): Promise<number> => {
    const db = requireDb()
    const links = await db
      .select()
      .from(haexPasswordsItemTags)
      .where(eq(haexPasswordsItemTags.tagId, tagId))
    const idSet = new Set(itemIds)
    let removed = 0
    for (const link of links) {
      if (!idSet.has(link.itemId)) continue
      await db
        .delete(haexPasswordsItemTags)
        .where(eq(haexPasswordsItemTags.id, link.id))
      removed++
    }
    return removed
  }

  return {
    tags,
    loadTagsAsync,
    getOrCreateTagAsync,
    resolveTagNamesAsync,
    setItemTagsAsync,
    getItemCountsAsync,
    renameAsync,
    updateColorAsync,
    deleteAsync,
    bulkAddTagAsync,
    bulkRemoveTagAsync,
  }
})
