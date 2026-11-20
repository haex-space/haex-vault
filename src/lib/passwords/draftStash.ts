// Keeps the unsaved draft of the entry editor while its tab shows another place (the generator, the
// import, ...): the editor is unmounted then and its form state would be gone on the way back.
// One draft per tab and entry (`new` for a new one). Pure and generic, so
// `scripts/check-passwords-draft-stash.ts` runs it without vue or the generated bindings.

export type DraftStash<T> = {
  /** Remembers `value`, replacing an older one of the same tab and entry. */
  keep(tabId: string, editKey: string, value: T): void
  /** The remembered value, which is forgotten with it; `null` if there is none. */
  take(tabId: string, editKey: string): T | null
  /** Forgets every draft of a tab (it closed). */
  dropTab(tabId: string): void
}

export function createDraftStash<T>(): DraftStash<T> {
  const byTab = new Map<string, Map<string, T>>()
  return {
    keep(tabId, editKey, value) {
      const drafts = byTab.get(tabId) ?? new Map<string, T>()
      drafts.set(editKey, value)
      byTab.set(tabId, drafts)
    },
    take(tabId, editKey) {
      const drafts = byTab.get(tabId)
      const value = drafts?.get(editKey)
      if (value === undefined) return null
      drafts?.delete(editKey)
      return value
    },
    dropTab(tabId) {
      byTab.delete(tabId)
    },
  }
}
