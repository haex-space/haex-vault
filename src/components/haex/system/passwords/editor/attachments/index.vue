<template>
  <div class="space-y-4">
    <!-- Existing + pending attachments -->
    <div
      v-if="allAttachments.length"
      class="space-y-4"
    >
      <div class="grid grid-cols-1 @xl:grid-cols-2 @3xl:grid-cols-3 gap-2">
        <div
          v-for="attachment in allAttachments"
          :key="attachment.id"
          class="flex items-center gap-2 p-3 border border-default rounded-lg transition-colors"
          :class="[
            editingId === attachment.id
              ? 'bg-elevated'
              : 'cursor-pointer hover:bg-elevated/50',
            isPending(attachment) ? 'border-dashed opacity-75' : '',
          ]"
          @click="editingId !== attachment.id ? onItemClick(attachment) : null"
        >
          <!-- Thumbnail for images -->
          <div
            v-if="isImage(attachment.fileName) && imageDataUrls[attachment.id]"
            class="size-16 rounded overflow-hidden shrink-0"
          >
            <img
              :src="imageDataUrls[attachment.id]"
              :alt="attachment.fileName"
              class="size-full object-cover"
            />
          </div>
          <!-- Placeholder while image loads -->
          <div
            v-else-if="isImage(attachment.fileName)"
            class="size-16 rounded bg-elevated shrink-0 flex items-center justify-center"
          >
            <UIcon
              name="i-lucide-image"
              class="size-5 text-muted"
            />
          </div>
          <!-- Icon for non-images -->
          <UIcon
            v-else-if="getFileType(attachment.fileName) === 'pdf'"
            name="i-lucide-file-text"
            class="size-5 text-error shrink-0"
          />
          <UIcon
            v-else-if="getFileType(attachment.fileName) === 'text'"
            name="i-lucide-file-code"
            class="size-5 text-primary shrink-0"
          />
          <UIcon
            v-else
            name="i-lucide-file"
            class="size-5 text-muted shrink-0"
          />

          <div class="flex-1 min-w-0">
            <input
              v-if="editingId === attachment.id"
              v-model="editingName"
              class="text-sm font-medium w-full bg-background border border-default rounded px-2 py-1"
              @click.stop
              @keyup.enter="saveRename(attachment)"
              @keyup.esc="cancelRename"
            />
            <template v-else>
              <p class="text-sm font-medium truncate">
                {{ attachment.fileName }}
              </p>
              <p
                v-if="attachment.size"
                class="text-xs text-muted"
              >
                {{ formatFileSize(attachment.size) }}
              </p>
            </template>
          </div>

          <!-- Rename mode buttons -->
          <template v-if="!readOnly && editingId === attachment.id">
            <UiButton
              icon="i-lucide-check"
              color="neutral"
              variant="ghost"
              type="button"
              @click.stop="saveRename(attachment)"
            />
            <UiButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              type="button"
              @click.stop="cancelRename"
            />
          </template>

          <!-- Normal mode buttons -->
          <template v-else>
            <template v-if="!readOnly">
              <UiButton
                icon="i-lucide-pencil"
                color="neutral"
                variant="ghost"
                type="button"
                @click.stop="startRename(attachment)"
              />
              <UiButton
                icon="i-lucide-trash-2"
                color="error"
                variant="ghost"
                type="button"
                @click.stop="remove(attachment)"
              />
            </template>
            <UiButton
              icon="i-lucide-download"
              color="neutral"
              variant="ghost"
              type="button"
              @click.stop="downloadAttachment(attachment)"
            />
          </template>
        </div>
      </div>
    </div>

    <!-- Empty state -->
    <div
      v-if="!allAttachments.length"
      class="flex flex-col items-center justify-center gap-2 py-10 text-muted"
    >
      <UIcon
        name="i-lucide-paperclip"
        class="size-10 opacity-40"
      />
      <p class="text-sm">
        {{ t('empty') }}
      </p>
    </div>

    <!-- Upload button -->
    <div v-if="!readOnly">
      <input
        ref="fileInput"
        type="file"
        multiple
        class="hidden"
        @change="onFileChange"
      />
      <UiButton
        icon="i-lucide-plus"
        :label="t('add')"
        color="neutral"
        variant="outline"
        type="button"
        @click="fileInput?.click()"
      />
    </div>

    <!-- Viewer for PDFs and text files -->
    <HaexSystemPasswordsEditorAttachmentsViewer
      v-model:open="viewerOpen"
      :attachment="viewerAttachment"
      :file-type="viewerFileType"
      :data-url="viewerDataUrl"
      @download="downloadAttachment"
    />
  </div>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { save } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import { haexPasswordsBinaries } from '~/database/schemas'
import { requireDb } from '~/stores/vault'
import {
  getFileType,
  isImage,
  createDataUrl,
  formatFileSize,
  type FileType,
} from '~/utils/passwords/fileTypes'
import { openPhotoSwipe } from '~/composables/passwords/usePhotoSwipe'
import type { AttachmentWithSize } from '~/types/passwords/attachment'

defineProps<{
  readOnly?: boolean
}>()

const attachments = defineModel<AttachmentWithSize[]>({ default: () => [] })
const attachmentsToAdd = defineModel<AttachmentWithSize[]>('attachmentsToAdd', { default: () => [] })
const attachmentsToDelete = defineModel<AttachmentWithSize[]>('attachmentsToDelete', { default: () => [] })

const { t } = useI18n()
const toast = useToast()
const fileInput = ref<HTMLInputElement>()

// Viewer state for PDF/text
const viewerOpen = ref(false)
const viewerAttachment = ref<AttachmentWithSize | null>(null)
const viewerFileType = ref<FileType | null>(null)
const viewerDataUrl = ref<string | null>(null)

// Inline rename state
const editingId = ref<string | null>(null)
const editingName = ref('')

const allAttachments = computed(() => [
  ...attachments.value,
  ...attachmentsToAdd.value,
])

// Image thumbnail cache (id → data URL) — loads lazily for saved attachments
const imageDataUrls = reactive<Record<string, string>>({})

watch(
  allAttachments,
  (list) => {
    for (const att of list) {
      if (!isImage(att.fileName) || imageDataUrls[att.id]) continue
      loadDataUrl(att).then((url) => {
        if (url) imageDataUrls[att.id] = url
      })
    }
  },
  { immediate: true },
)

function isPending(attachment: AttachmentWithSize): boolean {
  return attachmentsToAdd.value.some((a) => a.id === attachment.id)
}

async function loadDataUrl(attachment: AttachmentWithSize): Promise<string | null> {
  if (attachment.data) return createDataUrl(attachment.data, attachment.fileName)
  if (!attachment.binaryHash) return null

  const db = requireDb()
  const rows = await db
    .select({ data: haexPasswordsBinaries.data })
    .from(haexPasswordsBinaries)
    .where(eq(haexPasswordsBinaries.hash, attachment.binaryHash))
    .limit(1)

  return rows[0]?.data ? createDataUrl(rows[0].data, attachment.fileName) : null
}

async function onItemClick(attachment: AttachmentWithSize) {
  const fileType = getFileType(attachment.fileName)

  if (fileType === 'image') {
    // Collect all images and open in PhotoSwipe gallery
    const images = allAttachments.value.filter((a) => isImage(a.fileName))
    const clickedIndex = images.findIndex((a) => a.id === attachment.id)

    const resolved = await Promise.all(
      images.map(async (img) => ({
        src: (await loadDataUrl(img)) ?? '',
        alt: img.fileName,
      })),
    ).then((items) => items.filter((item) => item.src !== ''))

    if (resolved.length) await openPhotoSwipe(resolved, clickedIndex)
    return
  }

  if (fileType === 'pdf' || fileType === 'text') {
    const dataUrl = await loadDataUrl(attachment)
    if (!dataUrl) return
    viewerAttachment.value = attachment
    viewerFileType.value = fileType
    viewerDataUrl.value = dataUrl
    viewerOpen.value = true
  }
}

async function downloadAttachment(attachment: AttachmentWithSize) {
  try {
    const dataUrl = await loadDataUrl(attachment)
    if (!dataUrl) {
      toast.add({ title: t('downloadError'), color: 'error', icon: 'i-lucide-alert-triangle' })
      return
    }

    const filePath = await save({ defaultPath: attachment.fileName })
    if (!filePath) return // user cancelled

    const base64 = dataUrl.split(',')[1] ?? dataUrl
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)

    await writeFile(filePath, bytes)
  } catch (error) {
    console.error('[Attachments] Download failed:', error)
    toast.add({
      title: t('downloadError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

function remove(attachment: AttachmentWithSize) {
  if (isPending(attachment)) {
    attachmentsToAdd.value = attachmentsToAdd.value.filter((a) => a.id !== attachment.id)
  } else {
    attachmentsToDelete.value = [...attachmentsToDelete.value, attachment]
    attachments.value = attachments.value.filter((a) => a.id !== attachment.id)
  }
}

function startRename(attachment: AttachmentWithSize) {
  editingId.value = attachment.id
  editingName.value = attachment.fileName
}

function saveRename(attachment: AttachmentWithSize) {
  const newName = editingName.value.trim()
  if (!newName) return
  attachment.fileName = newName
  editingId.value = null
  editingName.value = ''
}

function cancelRename() {
  editingId.value = null
  editingName.value = ''
}

async function onFileChange(event: Event) {
  const target = event.target as HTMLInputElement
  const files = Array.from(target.files ?? [])
  if (!files.length) return

  for (const file of files) {
    const reader = new FileReader()
    reader.onload = () => {
      attachmentsToAdd.value = [
        ...attachmentsToAdd.value,
        {
          id: crypto.randomUUID(),
          itemId: '',
          binaryHash: '',
          fileName: file.name,
          size: file.size,
          data: reader.result as string,
        },
      ]
    }
    reader.readAsDataURL(file)
  }

  if (fileInput.value) fileInput.value.value = ''
}
</script>

<i18n lang="yaml">
de:
  empty: Keine Anhänge vorhanden
  add: Anhang hinzufügen
  downloadError: Download fehlgeschlagen
en:
  empty: No attachments
  add: Add attachment
  downloadError: Download failed
</i18n>
