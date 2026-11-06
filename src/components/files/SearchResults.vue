<script setup lang="ts">
/**
 * The hits of a search (spec 044 FR-027): best first, each with the folder it lies in, relative to
 * where the search started. A folder opens; a file opens in its folder's viewer; "im Ordner
 * zeigen" goes to the folder that holds the hit.
 */
import type { Entry } from '@bindings/Entry'
import type { SearchHit } from '@bindings/SearchHit'
import type { SourceRef } from '@bindings/SourceRef'
import { parentPath } from '~/lib/files/state'

const props = defineProps<{
  source: SourceRef
  /** Where the search started. */
  root: string
  hits: SearchHit[]
  running: boolean
  truncated: boolean
  /** Folders of a storage searched so far. */
  dirs: number
  error: string | null
}>()

const emit = defineEmits<{
  open: [entry: Entry]
  reveal: [folder: string]
}>()

const { t } = useI18n()

/** The folder of a hit below the search's start, `.` for the start itself. */
function where(entry: Entry): string {
  const folder = parentPath(entry.path) ?? entry.path
  if (folder === props.root) return '.'
  return folder.startsWith(props.root)
    ? folder.slice(props.root.length).replace(/^[\\/]/, '')
    : folder
}

const errorText = computed(() => {
  if (!props.error) return ''
  const key = `files.error.${props.error}`
  return t(key) === key ? props.error : t(key)
})
</script>

<template>
  <div class="flex h-full min-h-0 flex-col" data-testid="files-search-results">
    <div
      class="flex h-8 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground"
    >
      <Icon
        v-if="running"
        name="lucide:loader"
        class="size-3.5 animate-spin"
        data-testid="files-search-running"
      />
      <span data-testid="files-search-count">{{
        t('files.search.count', { count: hits.length }, hits.length)
      }}</span>
      <span v-if="truncated">{{ t('files.search.truncated') }}</span>
      <span v-if="running && dirs" data-testid="files-search-dirs">{{
        t('files.search.dirs', { count: dirs }, dirs)
      }}</span>
    </div>
    <p v-if="error" class="p-6 text-center text-sm text-muted-foreground">
      {{ errorText }}
    </p>
    <p
      v-else-if="!running && hits.length === 0"
      class="p-6 text-center text-sm text-muted-foreground"
      data-testid="files-search-none"
    >
      {{ t('files.search.none') }}
    </p>
    <ul v-else class="min-h-0 flex-1 overflow-y-auto">
      <li
        v-for="hit in hits"
        :key="hit.entry.path"
        class="group flex items-center gap-3 px-3 py-1.5 text-sm hover:bg-accent"
      >
        <button
          type="button"
          class="flex min-w-0 flex-1 items-center gap-3 text-left"
          :data-testid="`files-hit-${hit.entry.name}`"
          @click="emit('open', hit.entry)"
        >
          <span class="size-5 shrink-0">
            <FilesEntryIcon :source="source" :entry="hit.entry" />
          </span>
          <span class="flex min-w-0 flex-col">
            <span class="truncate">{{ hit.entry.name }}</span>
            <span class="truncate text-xs text-muted-foreground">{{
              where(hit.entry)
            }}</span>
          </span>
        </button>
        <UiButton
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('files.search.reveal')"
          :tooltip="t('files.search.reveal')"
          :data-testid="`files-hit-reveal-${hit.entry.name}`"
          @click="emit('reveal', parentPath(hit.entry.path) ?? hit.entry.path)"
        >
          <Icon name="lucide:folder-search" class="size-4" />
        </UiButton>
      </li>
    </ul>
  </div>
</template>
