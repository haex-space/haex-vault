import { invoke } from '@tauri-apps/api/core'
import { watchDebounced } from '@vueuse/core'
import { ref } from 'vue'

/**
 * The bindings `ChatSearchHit` / `ChatSearchResult`, spelled out: `check:chat-state` imports this
 * file, and the scripts' module resolution cannot follow the bindings' extensionless imports.
 */
export type SearchHit = {
  threadId: string
  kind: 'agent' | 'direct' | 'group' | 'note'
  messageId: string
  at: number
  snippet: string
}
export type SearchResult = { hits: SearchHit[]; more: boolean }

/** Characters (Unicode scalar values) before messages are searched (spec 076 FR-011). */
export const SEARCH_MIN_CHARS = 2
/** Wait after the last key before the messages are searched. */
export const SEARCH_DEBOUNCE_MS = 250

/**
 * The message search of the chat sidebar (spec 076 US2): `text` is the search field; 250 ms after
 * the last key, from two characters on, `chat_search` runs and `hits` show its result. A result
 * that comes back after the text changed is dropped (spec Edge Cases). Names are filtered by the
 * sidebar itself, at once. A failed search of the current text goes to `onError` (the sidebar
 * shows it as a toast) and sets `failed`. `searchAsync` is the command, replaceable in
 * `check:chat-state`.
 */
export function useChatSearch(
  onError: (error: unknown) => void,
  searchAsync: (text: string) => Promise<SearchResult> = (text) =>
    invoke<SearchResult>('chat_search', { args: { text } }),
) {
  const text = ref('')
  const hits = ref<SearchHit[]>([])
  const more = ref(false)
  const searching = ref(false)
  const failed = ref(false)
  /** The text the shown result is for; `''` without one. */
  const searched = ref('')
  let request = 0

  function reset() {
    hits.value = []
    more.value = false
    searching.value = false
    failed.value = false
    searched.value = ''
  }

  async function run(value: string) {
    const id = ++request
    if ([...value.trim()].length < SEARCH_MIN_CHARS) {
      reset()
      return
    }
    const current = () => id === request && value === text.value
    searching.value = true
    try {
      const result = await searchAsync(value)
      if (!current()) return
      hits.value = result.hits
      more.value = result.more
      failed.value = false
      searched.value = value
    } catch (e: unknown) {
      if (!current()) return
      hits.value = []
      more.value = false
      failed.value = true
      searched.value = value
      onError(e)
    } finally {
      if (id === request) searching.value = false
    }
  }

  watchDebounced(text, run, { debounce: SEARCH_DEBOUNCE_MS })

  function clear() {
    text.value = ''
    request++
    reset()
  }

  return { text, hits, more, searching, failed, searched, clear }
}
