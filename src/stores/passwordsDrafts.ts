import { defineStore } from 'pinia'
import type { ItemDetail } from '@bindings/ItemDetail'
import type { Draft } from '~/lib/passwords/draft'
import { createDraftStash } from '~/lib/passwords/draftStash'

/** What the editor needs to continue where the user left it. */
export type StashedEditor = {
  detail: ItemDetail | null
  initial: Draft
  draft: Draft
}

/**
 * The unsaved drafts of the entry editor (`EntryEditor.vue`) while a tab shows another place, so a
 * detour to the generator does not lose what was typed. The logic is the pure module
 * `lib/passwords/draftStash.ts`. Like the form itself it lives in memory only: never persisted, and
 * dropped with the tab (`PasswordsApp.vue`).
 */
export const usePasswordsDraftsStore = defineStore('passwordsDrafts', () => {
  const stash = createDraftStash<StashedEditor>()
  return {
    keep: stash.keep,
    take: stash.take,
    dropTab: stash.dropTab,
  }
})
