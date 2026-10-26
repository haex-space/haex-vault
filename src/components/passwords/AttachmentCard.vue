<script setup lang="ts">
/**
 * One attachment as a card (spec 036, US6, FR-037, FR-039, FR-041): a thumbnail for an image, made
 * when the card comes into view, else a type icon; the name, the size and the type for every
 * attachment; buttons Speichern unter, Umbenennen (inline: Enter or the check confirms, Escape
 * cancels) and Entfernen, as in spec 034. A tap on the card opens an image in the lightbox, a text
 * or a PDF in the attachment view (spec 058 FR-001) and saves anything else. On the desktop the card
 * also offers "Mit System-App öffnen" (spec 058 FR-009).
 */
import type { AttachmentView } from '@bindings/AttachmentView'
import { fileKind, formatFileSize, type FileKind } from '~/lib/passwords/format'
import type { Thumbnail } from '~/lib/images/thumbnailCache'

const props = defineProps<{
  attachment: AttachmentView
  readonly?: boolean
  /** "Mit System-App öffnen" is offered (spec 058 FR-009). */
  systemApps?: boolean
}>()

const emit = defineEmits<{
  open: []
  view: []
  openSystem: []
  save: []
  rename: [fileName: string]
  remove: []
}>()

const { t } = useI18n()
const { storedThumbnail, thumbnailAsync } = usePasswordsThumbnails()

const KIND_ICONS: Record<FileKind, string> = {
  image: 'lucide:file-image',
  pdf: 'lucide:file-text',
  text: 'lucide:file-type',
  other: 'lucide:file',
}

const kind = computed(() => fileKind(props.attachment.fileName))
const typeLabel = computed(() => {
  const name = props.attachment.fileName
  const dot = name.lastIndexOf('.')
  return dot > 0 && dot < name.length - 1
    ? name.slice(dot + 1).toUpperCase()
    : t('passwords.attachments.kinds.other')
})

const card = useTemplateRef<HTMLElement>('card')
const thumbnail = ref<Thumbnail | null>(
  kind.value === 'image' ? (storedThumbnail(props.attachment) ?? null) : null,
)

async function loadThumbnailAsync() {
  if (kind.value !== 'image' || thumbnail.value) return
  // The cache asks once per checksum, however often this runs.
  const hash = props.attachment.binaryHash
  const result = await thumbnailAsync(props.attachment)
  if (props.attachment.binaryHash === hash && kind.value === 'image') {
    thumbnail.value = result
  }
}

// FR-041: only the cards in view render their thumbnails.
const seen = ref(false)
const { stop } = useIntersectionObserver(card, ([entry]) => {
  if (!entry?.isIntersecting) return
  stop()
  seen.value = true
  void loadThumbnailAsync()
})

// New bytes or a new name (a rename can turn a file into an image or back): the stored thumbnail
// of the new state, rendered when the card has been in view.
watch(
  () => [props.attachment.binaryHash, kind.value] as const,
  () => {
    thumbnail.value =
      kind.value === 'image'
        ? (storedThumbnail(props.attachment) ?? null)
        : null
    if (seen.value) void loadThumbnailAsync()
  },
)

const editing = ref(false)
const fileName = ref('')

function startRename() {
  fileName.value = props.attachment.fileName
  editing.value = true
}

function confirmRename() {
  const name = fileName.value.trim()
  if (!name) return
  editing.value = false
  if (name !== props.attachment.fileName) emit('rename', name)
}

function activate() {
  if (kind.value === 'image') emit('open')
  else if (kind.value === 'text' || kind.value === 'pdf') emit('view')
  else emit('save')
}
</script>

<template>
  <div
    ref="card"
    class="flex min-w-0 flex-col overflow-hidden rounded-xl border bg-card"
    :data-testid="`passwords-attachment-card-${attachment.id}`"
    :data-kind="kind"
  >
    <button
      type="button"
      class="group flex aspect-[4/3] items-center justify-center bg-muted/60 outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :aria-label="
        kind !== 'other'
          ? t('passwords.attachments.open', { name: attachment.fileName })
          : t('passwords.attachments.saveAs', { name: attachment.fileName })
      "
      :data-testid="`passwords-attachment-open-${attachment.id}`"
      @click="activate"
    >
      <img
        v-if="thumbnail?.kind === 'ready'"
        :src="thumbnail.url"
        alt=""
        class="size-full object-cover transition-opacity group-hover:opacity-90"
        :data-testid="`passwords-attachment-thumbnail-${attachment.id}`"
      />
      <span
        v-else-if="kind === 'image' && thumbnail === null"
        class="size-8 animate-pulse rounded-md bg-muted-foreground/20"
        :aria-label="t('passwords.attachments.loading')"
      />
      <span v-else class="flex flex-col items-center gap-1 px-2 text-center">
        <Icon
          :name="KIND_ICONS[kind]"
          class="size-10 text-muted-foreground"
          :data-testid="`passwords-attachment-icon-${attachment.id}`"
        />
        <span
          v-if="kind === 'image'"
          class="text-xs text-muted-foreground"
          :data-testid="`passwords-attachment-unreadable-${attachment.id}`"
        >
          {{ t('passwords.attachments.noPreview') }}
        </span>
      </span>
    </button>

    <div class="flex min-w-0 flex-col gap-1 p-2">
      <div v-if="editing" class="flex items-center gap-1">
        <UiInput
          v-model="fileName"
          class="min-w-0 flex-1"
          :aria-label="t('passwords.attachments.name')"
          :data-testid="`passwords-attachment-name-${attachment.id}`"
          @keydown.enter.prevent="confirmRename"
          @keydown.esc.prevent.stop="editing = false"
        />
        <UiButton
          type="button"
          variant="ghost"
          size="icon-sm"
          :aria-label="t('passwords.save')"
          :data-testid="`passwords-attachment-name-confirm-${attachment.id}`"
          @click="confirmRename"
        >
          <Icon name="lucide:check" class="size-4" />
        </UiButton>
      </div>
      <template v-else>
        <span
          class="truncate text-sm font-medium"
          :title="attachment.fileName"
          :data-testid="`passwords-attachment-title-${attachment.id}`"
          >{{ attachment.fileName }}</span
        >
        <span
          class="truncate text-xs text-muted-foreground"
          :data-testid="`passwords-attachment-meta-${attachment.id}`"
          >{{ formatFileSize(attachment.size) }} · {{ typeLabel }}</span
        >
      </template>
      <div class="flex items-center justify-end gap-0.5">
        <UiButton
          v-if="systemApps"
          type="button"
          variant="ghost"
          size="icon-sm"
          :aria-label="t('passwords.attachments.openSystem')"
          :tooltip="t('passwords.attachments.openSystem')"
          :data-testid="`passwords-attachment-open-system-${attachment.id}`"
          @click="emit('openSystem')"
        >
          <Icon name="lucide:external-link" class="size-4" />
        </UiButton>
        <UiButton
          type="button"
          variant="ghost"
          size="icon-sm"
          :aria-label="t('passwords.attachments.download')"
          :tooltip="t('passwords.attachments.download')"
          :data-testid="`passwords-attachment-download-${attachment.id}`"
          @click="emit('save')"
        >
          <Icon name="lucide:download" class="size-4" />
        </UiButton>
        <template v-if="!readonly">
          <UiButton
            type="button"
            variant="ghost"
            size="icon-sm"
            :aria-label="t('passwords.attachments.rename')"
            :tooltip="t('passwords.attachments.rename')"
            :data-testid="`passwords-attachment-rename-${attachment.id}`"
            @click="startRename"
          >
            <Icon name="lucide:pencil" class="size-4" />
          </UiButton>
          <UiButton
            type="button"
            variant="ghost"
            size="icon-sm"
            :aria-label="t('passwords.attachments.remove')"
            :tooltip="t('passwords.attachments.remove')"
            :data-testid="`passwords-attachment-remove-${attachment.id}`"
            @click="emit('remove')"
          >
            <Icon name="lucide:trash-2" class="size-4" />
          </UiButton>
        </template>
      </div>
    </div>
  </div>
</template>
