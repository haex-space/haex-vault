// Drag and drop between the list and the sidebar (spec 034, US2): what travels is ids only, never a
// title or a value. Pure, so it is checked without a browser.

/** A folder dragged inside the sidebar tree. */
export const FOLDER_MIME = 'application/x-haex-passwords-folder'
/** Entries dragged from the list onto a folder. */
export const ITEMS_MIME = 'application/x-haex-passwords-items'

/** The text that carries the dragged entries. */
export function itemsPayload(ids: readonly string[]): string {
  return JSON.stringify(ids)
}

/** The entry ids in a drop, or `[]` when the text is not ours. */
export function parseItemsPayload(text: string | undefined | null): string[] {
  if (!text) return []
  try {
    const parsed: unknown = JSON.parse(text)
    return Array.isArray(parsed) &&
      parsed.every((id) => typeof id === 'string' && id.length > 0)
      ? (parsed as string[])
      : []
  } catch {
    return []
  }
}

/** What a drag of `id` carries: the whole selection when the dragged entry is part of it, else the
 * entry alone. */
export function draggedIds(id: string, selected: readonly string[]): string[] {
  return selected.includes(id) ? [...selected] : [id]
}
