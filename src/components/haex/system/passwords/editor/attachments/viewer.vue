<template>
  <UModal
    v-model:open="open"
    :title="attachment?.fileName ?? ''"
    :ui="{ content: 'max-w-4xl' }"
  >
    <template #body>
      <div class="max-h-[70vh] overflow-auto">
        <!-- Image -->
        <img
          v-if="fileType === 'image' && dataUrl"
          :src="dataUrl"
          :alt="attachment?.fileName"
          class="max-w-full mx-auto rounded"
        />

        <!-- PDF via blob URL (data: URLs blocked by CSP in Tauri WebView) -->
        <div
          v-else-if="fileType === 'pdf' && pdfBlobUrl"
          class="h-[65vh]"
        >
          <embed
            :src="pdfBlobUrl"
            type="application/pdf"
            class="w-full h-full"
          />
        </div>

        <!-- Text -->
        <div
          v-else-if="fileType === 'text' && textContent !== null"
          class="p-4 bg-elevated rounded-md"
        >
          <pre class="text-sm whitespace-pre-wrap break-words font-mono">{{ textContent }}</pre>
        </div>

        <!-- Unsupported -->
        <div
          v-else
          class="text-center py-10 text-muted"
        >
          {{ t('previewNotAvailable') }}
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex justify-end w-full">
        <UiButton
          v-if="attachment"
          icon="i-lucide-download"
          :label="t('download')"
          color="neutral"
          variant="outline"
          @click="$emit('download', attachment)"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type { AttachmentWithSize } from '~/types/passwords/attachment'

const props = defineProps<{
  attachment: AttachmentWithSize | null
  fileType: import('~/utils/passwords/fileTypes').FileType | null
  dataUrl: string | null
}>()

defineEmits<{
  download: [attachment: AttachmentWithSize]
}>()

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()

const pdfBlobUrl = ref<string | null>(null)

watchEffect(() => {
  if (pdfBlobUrl.value) {
    URL.revokeObjectURL(pdfBlobUrl.value)
    pdfBlobUrl.value = null
  }

  if (props.fileType !== 'pdf' || !props.dataUrl) return

  try {
    const base64 = props.dataUrl.split(',')[1] ?? props.dataUrl
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
    pdfBlobUrl.value = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }))
  } catch (error) {
    console.error('[AttachmentViewer] PDF blob creation failed:', error)
  }
})

onUnmounted(() => {
  if (pdfBlobUrl.value) URL.revokeObjectURL(pdfBlobUrl.value)
})

const textContent = computed<string | null>(() => {
  if (props.fileType !== 'text' || !props.dataUrl) return null
  try {
    const base64 = props.dataUrl.split(',')[1] ?? props.dataUrl
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
    return new TextDecoder('utf-8').decode(bytes)
  } catch (error) {
    console.error('[AttachmentViewer] Text decode failed:', error)
    return null
  }
})
</script>

<i18n lang="yaml">
de:
  previewNotAvailable: Vorschau nicht verfügbar
  download: Herunterladen
en:
  previewNotAvailable: Preview not available
  download: Download
</i18n>
