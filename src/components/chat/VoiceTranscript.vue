<script setup lang="ts">
/**
 * The transcript of a spoken conversation (spec 066 T127, FR-047, FR-048): everything said in
 * both directions with its time, speaker and kind, answers that were interrupted greyed from where
 * haex stopped. Shown instead of the messages while "Mitschrift" is on in the session header.
 * Reads `assistant_transcript_list`, adds entries from `assistant-transcript` as they are stored,
 * and reads again when a synced change arrives. `focus` scrolls to a search hit.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { nextTick, onBeforeUnmount, ref, useTemplateRef, watch } from 'vue'
import { toast } from 'vue-sonner'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import {
  LABELLED_KINDS,
  OUTCOMES,
  SPEAKERS,
  mergeEntry,
  mergeStored,
  splitSpoken,
  type TranscriptEntry,
} from '~/lib/assistant/transcript'

const props = defineProps<{
  threadId: string
  /** The entry to scroll to and mark, from a search hit; `request` changes with every selection,
   * so the same entry can be asked for again. */
  focus: { id: string; request: number } | null
}>()

const { t, locale } = useI18n()
const { errString } = useErrorString()

const entries = ref<TranscriptEntry[]>([])
const loading = ref(true)
const root = useTemplateRef<HTMLElement>('root')

let generation = 0
async function loadAsync() {
  const thread = props.threadId
  const mine = ++generation
  try {
    const stored = await invoke<TranscriptEntry[]>(
      'assistant_transcript_list',
      { threadId: thread },
    )
    if (mine !== generation) return
    entries.value = mergeStored(entries.value, stored)
  } catch (error) {
    if (mine === generation)
      toast.error(
        t('assistant.transcript.loadFailed', { reason: errString(error) }),
      )
  } finally {
    if (mine === generation) loading.value = false
  }
  await scrollToFocusAsync()
}

watch(
  () => props.threadId,
  () => {
    entries.value = []
    loading.value = true
    void loadAsync()
  },
  { immediate: true },
)
onVaultTablesChanged(['voice_transcript_entries'], () => loadAsync())

let unlisten: UnlistenFn | null = null
let disposed = false
void listen<TranscriptEntry>('assistant-transcript', (event) => {
  if (event.payload.threadId === props.threadId)
    entries.value = mergeEntry(entries.value, event.payload)
}).then((stop) => {
  if (disposed) stop()
  else unlisten = stop
})
onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
})

// Each selection scrolls once, when its entry is on the page: a read after a synced change must
// not pull the reader back from where they scrolled to.
let scrolledRequest = -1
async function scrollToFocusAsync() {
  const focus = props.focus
  if (focus === null || focus.request === scrolledRequest) return
  await nextTick()
  const target = root.value?.querySelector(
    `[data-id="${CSS.escape(focus.id)}"]`,
  )
  if (!target) return
  scrolledRequest = focus.request
  target.scrollIntoView({ block: 'center' })
}
watch(() => props.focus, scrollToFocusAsync)

function speakerName(speaker: string): string {
  return SPEAKERS.includes(speaker)
    ? t(`assistant.speaker.${speaker}`)
    : speaker
}

function time(at: number): string {
  return new Date(at).toLocaleTimeString(locale.value, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function fullTime(at: number): string {
  return new Date(at).toLocaleString(locale.value, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  })
}
</script>

<template>
  <section
    ref="root"
    class="mx-auto flex w-full max-w-3xl flex-col gap-3"
    data-testid="voice-transcript"
    :aria-label="t('assistant.transcript.title')"
  >
    <p
      v-if="!loading && entries.length === 0"
      class="text-sm text-muted-foreground"
    >
      {{ t('assistant.transcript.empty') }}
    </p>
    <ol class="flex flex-col gap-3" aria-live="polite">
      <li
        v-for="entry in entries"
        :key="entry.id"
        class="flex flex-col gap-1 rounded-lg px-3 py-2"
        :class="[
          entry.speaker === 'user' ? 'bg-muted/60' : '',
          entry.id === focus?.id ? 'ring-2 ring-primary' : '',
        ]"
        :data-id="entry.id"
        :data-testid="`voice-transcript-entry-${entry.kind}`"
      >
        <div
          class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground"
        >
          <time
            class="tabular-nums"
            :datetime="new Date(entry.spokenAt).toISOString()"
            :title="fullTime(entry.spokenAt)"
          >
            {{ time(entry.spokenAt) }}
          </time>
          <span class="font-medium text-foreground">
            {{ speakerName(entry.speaker) }}
          </span>
          <span
            v-if="LABELLED_KINDS.includes(entry.kind)"
            class="rounded-full border px-1.5 leading-4"
          >
            {{ t(`assistant.kind.${entry.kind}`) }}
          </span>
          <span
            v-if="entry.outcome && OUTCOMES.includes(entry.outcome)"
            class="rounded-full bg-muted px-1.5 leading-4"
          >
            {{ t(`assistant.outcome.${entry.outcome}`) }}
          </span>
        </div>
        <p class="whitespace-pre-wrap text-sm">
          {{ splitSpoken(entry.text, entry.spokenUntil).spoken
          }}<span
            v-if="splitSpoken(entry.text, entry.spokenUntil).unspoken"
            class="text-muted-foreground/60"
            :title="t('assistant.transcript.unspoken')"
            data-testid="voice-transcript-unspoken"
            ><span class="sr-only"
              >{{ t('assistant.transcript.unspoken') }}: </span
            >{{ splitSpoken(entry.text, entry.spokenUntil).unspoken }}</span
          >
        </p>
      </li>
    </ol>
  </section>
</template>
