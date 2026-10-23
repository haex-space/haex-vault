<script setup lang="ts">
/**
 * The viewer over the open folder (spec 044 FR-009, FR-011, FR-014, FR-015): text shows here;
 * images, video, audio and PDFs come from the media server (FR-016: the URL is released when the
 * viewer moves on or closes). A file the web view cannot play falls back to the info view.
 * Arrows and the buttons move to the previous or next file of the same kind; Escape closes.
 */
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import type { TextContent } from '@bindings/TextContent'
import { asFilesError } from '~/composables/useFiles'
import { formatFileSize } from '~/lib/passwords/format'
import { viewerKind } from '~/lib/files/viewerKind'

const props = defineProps<{
  source: SourceRef
  entry: Pick<Entry, 'name' | 'path'> & Partial<Entry>
  siblings: Entry[]
}>()

const emit = defineEmits<{
  close: []
  show: [entry: Entry]
  /** Open with the system's app; the frame asks first for a file of an encrypted folder. */
  openSystem: [path: string]
}>()

const { t, d } = useI18n()
const { readTextAsync, openAsync, releaseAsync } = useFiles()
const { isAndroid } = useDeviceCapabilities()
const { tabId } = useWmTab()

const kind = computed(() => viewerKind(props.entry.name))
/** Set when the web view could not play or render the file. */
const unplayable = ref(false)
/** What the viewer shows; the rest goes to the info view. */
const shown = computed(() => {
  if (unplayable.value) return 'info'
  switch (kind.value) {
    case 'text':
    case 'image':
    case 'pdf':
    case 'video':
    case 'audio':
      return kind.value
    default:
      return 'info'
  }
})

const text = ref<TextContent | null>(null)
const url = ref<string | null>(null)
const failed = ref<string | null>(null)
const zoomed = ref(false)

function dropUrl() {
  const current = url.value
  url.value = null
  if (current) void releaseAsync(current).catch(() => {})
}

watch(
  () => props.entry.path,
  async (path) => {
    text.value = null
    dropUrl()
    failed.value = null
    zoomed.value = false
    unplayable.value = false
    try {
      if (shown.value === 'text') {
        const content = await readTextAsync(props.source, path)
        if (path === props.entry.path) text.value = content
      } else if (shown.value !== 'info') {
        const opened = await openAsync(props.source, path, tabId)
        if (path === props.entry.path) url.value = opened.url
        else if (opened.url) void releaseAsync(opened.url).catch(() => {})
      }
    } catch (error) {
      if (path !== props.entry.path) return
      const code = asFilesError(error)?.code
      failed.value = code ? t(`files.error.${code}`) : String(error)
    }
  },
  { immediate: true },
)
onBeforeUnmount(dropUrl)

/** The files of the same kind in folder order, for previous and next. */
const sameKind = computed(() =>
  props.siblings.filter(
    (sibling) =>
      sibling.kind === 'file' && viewerKind(sibling.name) === kind.value,
  ),
)
const position = computed(() =>
  sameKind.value.findIndex((sibling) => sibling.path === props.entry.path),
)

function step(by: number) {
  const list = sameKind.value
  if (list.length < 2 || position.value < 0) return
  const next = list[(position.value + by + list.length) % list.length]
  if (next) emit('show', next)
}

function onKeydown(event: KeyboardEvent) {
  // The arrows of a focused player seek; they do not move to another file.
  if (event.target instanceof HTMLMediaElement && event.key !== 'Escape') return
  if (event.key === 'Escape') emit('close')
  else if (event.key === 'ArrowLeft') step(-1)
  else if (event.key === 'ArrowRight') step(1)
}

function openSystem() {
  emit('openSystem', props.entry.path)
}

const panel = useTemplateRef<HTMLElement>('panel')
onMounted(() => panel.value?.focus())
</script>

<template>
  <div
    ref="panel"
    class="absolute inset-0 z-30 flex flex-col bg-background outline-none"
    tabindex="-1"
    role="dialog"
    :aria-label="entry.name"
    data-testid="files-viewer"
    @keydown="onKeydown"
  >
    <div class="flex h-12 shrink-0 items-center gap-1 border-b px-2">
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('files.viewer.close')"
        :tooltip="t('files.viewer.close')"
        data-testid="files-viewer-close"
        @click="emit('close')"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
      <span class="min-w-0 flex-1 truncate text-sm font-medium">{{
        entry.name
      }}</span>
      <template v-if="sameKind.length > 1">
        <UiButton
          variant="ghost"
          size="icon"
          :aria-label="t('files.viewer.previous')"
          :tooltip="t('files.viewer.previous')"
          data-testid="files-viewer-previous"
          @click="step(-1)"
        >
          <Icon name="lucide:chevron-left" class="size-4" />
        </UiButton>
        <UiButton
          variant="ghost"
          size="icon"
          :aria-label="t('files.viewer.next')"
          :tooltip="t('files.viewer.next')"
          data-testid="files-viewer-next"
          @click="step(1)"
        >
          <Icon name="lucide:chevron-right" class="size-4" />
        </UiButton>
      </template>
      <UiButton
        v-if="!isAndroid"
        variant="ghost"
        size="icon"
        :aria-label="t('files.viewer.openSystem')"
        :tooltip="t('files.viewer.openSystem')"
        data-testid="files-viewer-open-system"
        @click="openSystem"
      >
        <Icon name="lucide:external-link" class="size-4" />
      </UiButton>
    </div>

    <div class="relative min-h-0 flex-1 overflow-auto">
      <p
        v-if="failed"
        class="p-6 text-center text-sm text-muted-foreground"
        data-testid="files-viewer-failed"
      >
        {{ failed }}
      </p>
      <div v-else-if="shown === 'text' && text" data-testid="files-viewer-text">
        <FilesTextContent :text="text" />
      </div>
      <div
        v-else-if="shown === 'image' && url"
        class="flex min-h-full items-center justify-center p-2"
        data-testid="files-viewer-image"
      >
        <img
          :src="url"
          :alt="entry.name"
          class="cursor-zoom-in"
          :class="
            zoomed
              ? 'max-w-none cursor-zoom-out'
              : 'max-h-full max-w-full object-contain'
          "
          @click="zoomed = !zoomed"
        />
      </div>
      <FilesMediaViewer
        v-else-if="(shown === 'video' || shown === 'audio') && url"
        :url="url"
        :kind="shown"
        @failed="unplayable = true"
      />
      <LazyFilesPdfViewer
        v-else-if="shown === 'pdf' && url"
        :url="url"
        @failed="unplayable = true"
      />
      <p
        v-if="unplayable"
        class="border-b bg-muted px-4 py-2 text-center text-xs text-muted-foreground"
        data-testid="files-viewer-unplayable"
      >
        {{ t('files.viewer.unplayable') }}
      </p>
      <dl
        v-if="shown === 'info' && !failed"
        class="mx-auto grid max-w-md grid-cols-[auto_1fr] gap-x-4 gap-y-2 p-6 text-sm"
        data-testid="files-viewer-info"
      >
        <dt class="text-muted-foreground">{{ t('files.info.name') }}</dt>
        <dd class="break-all">{{ entry.name }}</dd>
        <template v-if="entry.mime">
          <dt class="text-muted-foreground">{{ t('files.info.type') }}</dt>
          <dd>{{ entry.mime }}</dd>
        </template>
        <template v-if="entry.size !== undefined && entry.size !== null">
          <dt class="text-muted-foreground">{{ t('files.info.size') }}</dt>
          <dd>{{ formatFileSize(entry.size) }}</dd>
        </template>
        <template v-if="entry.modifiedMs">
          <dt class="text-muted-foreground">{{ t('files.info.modified') }}</dt>
          <dd>
            {{
              d(new Date(entry.modifiedMs), {
                dateStyle: 'medium',
                timeStyle: 'short',
              })
            }}
          </dd>
        </template>
        <dd v-if="!isAndroid" class="col-span-2 pt-4">
          <UiButton data-testid="files-info-open-system" @click="openSystem">
            <Icon name="lucide:external-link" class="size-4" />
            {{ t('files.viewer.openSystem') }}
          </UiButton>
        </dd>
      </dl>
    </div>
  </div>
</template>
