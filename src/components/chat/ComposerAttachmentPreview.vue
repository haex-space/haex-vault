<script setup lang="ts">
/**
 * The preview of an attached image (spec 011 FR-020): the whole image in a dialog (a drawer on a
 * small screen). The bytes come from the backend (`read_attachment_preview`) the way they are read
 * at send time, so a file chosen on Android previews too. The object URL is given back when the
 * preview closes, also when the load ends after that.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { ComposerAttachment } from '~/components/chat/ComposerAttachments.vue'
import { imageMime } from '~/lib/passwords/format'

const props = defineProps<{
  attachment: ComposerAttachment | null
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { errString } = useErrorString()

const url = ref<string | null>(null)
let loadId = 0

function release() {
  if (url.value) URL.revokeObjectURL(url.value)
  url.value = null
}

watch(
  () => (open.value ? props.attachment : null),
  async (attachment) => {
    const id = ++loadId
    release()
    if (!attachment) return
    try {
      const bytes = await invoke<ArrayBuffer>('read_attachment_preview', {
        path: attachment.path,
      })
      if (id !== loadId) return
      const type = imageMime(attachment.info.name) ?? 'image/png'
      url.value = URL.createObjectURL(new Blob([bytes], { type }))
    } catch (e: unknown) {
      if (id !== loadId) return
      open.value = false
      toast.error(errString(e))
    }
  },
)

onBeforeUnmount(() => {
  loadId++
  release()
})
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="attachment?.info.name ?? ''"
    content-class="sm:max-w-3xl"
  >
    <template #content>
      <div
        class="flex min-h-24 items-center justify-center"
        :aria-busy="!url"
        data-testid="chat-attachment-preview"
      >
        <img
          v-if="url"
          :src="url"
          :alt="attachment?.info.name ?? ''"
          class="max-h-[70vh] max-w-full rounded-md object-contain"
        />
        <Icon
          v-else
          name="lucide:loader-2"
          class="h-6 w-6 animate-spin text-muted-foreground"
          :aria-label="t('chat.composer.attachments.loadingPreview')"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>
