/**
 * Searching the transcripts of spoken conversations from "Verlauf" (spec 066 T127, FR-048): a
 * debounced query against `assistant_transcript_search`, a page at a time. A newer query drops the
 * answers of older ones.
 */
import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'
import type {
  TranscriptHit,
  TranscriptSearchPage,
} from '~/lib/assistant/transcript'

export function useTranscriptSearch() {
  const query = ref('')
  const hits = ref<TranscriptHit[]>([])
  const nextCursor = ref<string | null>(null)
  const searching = ref(false)
  const error = ref<unknown>(null)
  let generation = 0

  async function fetchAsync(cursor: string | null) {
    const text = query.value.trim()
    const mine = ++generation
    if (text === '') {
      hits.value = []
      nextCursor.value = null
      searching.value = false
      return
    }
    searching.value = true
    error.value = null
    try {
      const page = await invoke<TranscriptSearchPage>(
        'assistant_transcript_search',
        { query: text, cursor },
      )
      if (mine !== generation) return
      hits.value =
        cursor === null ? page.entries : [...hits.value, ...page.entries]
      nextCursor.value = page.nextCursor ?? null
    } catch (failure) {
      if (mine === generation) error.value = failure
    } finally {
      if (mine === generation) searching.value = false
    }
  }

  watchDebounced(query, () => fetchAsync(null), { debounce: 250 })

  return {
    query,
    hits,
    nextCursor,
    searching,
    error,
    /** The next page of the current query. */
    moreAsync: () =>
      nextCursor.value === null
        ? Promise.resolve()
        : fetchAsync(nextCursor.value),
  }
}
