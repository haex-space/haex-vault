<script setup lang="ts">
/**
 * The attachments of an entry (spec 034, US5, FR-019..FR-021; spec 036, US6, FR-037..FR-041): cards
 * in a grid of three, two or one columns by the width of the container; a tap on an image opens the
 * lightbox over the entry's images in card order, a tap on anything else saves it. Adding through
 * the system's file dialog or by dropping files, renaming, removing and saving work as in 034.
 * Files travel as the dialog's choice (spec 043: a path or an address), never through the webview;
 * nothing of the choice is stored. An attachment is saved
 * at once (it has no draft), so the editor can show this too.
 */
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { toast } from 'vue-sonner'
import type { AttachmentView } from '@bindings/AttachmentView'
import type { PickedFile } from '@bindings/PickedFile'
import { fileKind, safeFileName } from '~/lib/passwords/format'

const props = defineProps<{
  itemId: string
  attachments: AttachmentView[]
  /** An entry in the trash only shows its files. */
  readonly?: boolean
}>()

const emit = defineEmits<{
  changed: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const {
  attachmentAddAsync,
  attachmentRenameAsync,
  attachmentRemoveAsync,
  attachmentSaveAsync,
} = usePasswords()
const { pickManyAsync, pickSaveTargetAsync } = usePickedFile()

const root = ref<HTMLElement | null>(null)
const busy = ref(false)
const dragging = ref(false)
const lightbox = useTemplateRef<{ open: (index: number) => Promise<void> }>(
  'lightbox',
)

/** The images in card order: what the lightbox shows (FR-038). */
const images = computed(() =>
  props.attachments.filter(
    (attachment) => fileKind(attachment.fileName) === 'image',
  ),
)

/** Chosen or dropped files (a dropped file is a path, a chosen one a path or an address). */
async function addFilesAsync(files: PickedFile[]) {
  if (props.readonly || busy.value || !files.length) return
  busy.value = true
  try {
    // One attachment per call: each is one write of its own.
    for (const file of files) {
      try {
        await attachmentAddAsync(props.itemId, file)
      } catch (cause) {
        toast.error(errString(cause))
      }
    }
    emit('changed')
  } finally {
    busy.value = false
  }
}

async function pickAsync() {
  await addFilesAsync(await pickManyAsync())
}

async function downloadAsync(attachment: AttachmentView) {
  const target = await pickSaveTargetAsync(safeFileName(attachment.fileName))
  if (!target) return
  try {
    await attachmentSaveAsync(attachment.id, target)
    toast.success(t('passwords.attachments.saved'))
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function openImage(attachment: AttachmentView) {
  const index = images.value.findIndex((image) => image.id === attachment.id)
  if (index >= 0) void lightbox.value?.open(index)
}

async function renameAsync(attachment: AttachmentView, name: string) {
  if (props.readonly) return
  try {
    await attachmentRenameAsync(attachment.id, name)
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function removeAsync(attachment: AttachmentView) {
  if (props.readonly) return
  try {
    await attachmentRemoveAsync(attachment.id)
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  }
}

// Tauri hands dropped files over as paths, for the whole webview: only a drop whose position lies
// on this list counts.
let unlisten: (() => void) | undefined
function overList(position: { x: number; y: number }): boolean {
  const rect = root.value?.getBoundingClientRect()
  if (!rect) return false
  const x = position.x / window.devicePixelRatio
  const y = position.y / window.devicePixelRatio
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

onMounted(async () => {
  if (props.readonly) return
  try {
    unlisten = await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      if (payload.type === 'leave') {
        dragging.value = false
      } else if (payload.type === 'drop') {
        dragging.value = false
        if (overList(payload.position)) void addFilesAsync(payload.paths)
      } else {
        dragging.value = overList(payload.position)
      }
    })
  } catch {
    // No webview (a preview in a browser): the add button still works.
  }
})

onBeforeUnmount(() => {
  unlisten?.()
})
</script>

<template>
  <div ref="root" data-testid="passwords-attachments">
    <SettingsGroup :label="t('passwords.attachments.title')">
      <div
        v-if="attachments.length"
        class="@container/attachments p-2"
        data-testid="passwords-attachment-grid"
      >
        <ul
          class="grid grid-cols-1 gap-2 @xs/attachments:grid-cols-2 @lg/attachments:grid-cols-3"
          role="list"
        >
          <li v-for="attachment in attachments" :key="attachment.id">
            <PasswordsAttachmentCard
              :attachment="attachment"
              :readonly="readonly"
              @open="openImage(attachment)"
              @save="downloadAsync(attachment)"
              @rename="(name: string) => renameAsync(attachment, name)"
              @remove="removeAsync(attachment)"
            />
          </li>
        </ul>
      </div>
      <SettingsRow
        v-if="!readonly"
        :class="dragging ? 'bg-foreground/10' : ''"
        :title="
          attachments.length
            ? t('passwords.attachments.addMore')
            : t('passwords.attachments.empty')
        "
        :description="t('passwords.attachments.hint')"
        icon="lucide:file-plus"
      >
        <UiButton
          type="button"
          size="sm"
          :disabled="busy"
          data-testid="passwords-attachment-add"
          @click="pickAsync"
        >
          {{ t('passwords.attachments.add') }}
        </UiButton>
      </SettingsRow>
    </SettingsGroup>
    <PasswordsAttachmentLightbox
      ref="lightbox"
      :images="images"
      @save="downloadAsync"
    />
  </div>
</template>
