<script setup lang="ts">
/**
 * A file of a message in a contact conversation (spec 072 US3, FR-036, FR-037): name, size and type
 * icon, and by the state of this device's copy what follows: the progress while it is coming,
 * "Herunterladen" above the automatic limit, the reason and a new attempt after a failure, or the
 * hint that the file is gone from the server. The sender's own files are on this device at once.
 * A file on this device opens with the system's app on a computer (scenario 4), through the same
 * notice about the decrypted copy as the file browser and the password manager.
 */
import type { ChatPeerAttachment } from '@bindings/ChatPeerAttachment'
import { textControl } from '~/lib/chat/longText'
import { viewerKind } from '~/lib/files/viewerKind'
import { formatFileSize } from '~/lib/passwords/format'

const props = defineProps<{
  file: ChatPeerAttachment
  /** Written by this vault: the file is here, whatever the transfer state says. */
  own: boolean
}>()

const emit = defineEmits<{
  /** Fetch this file now (above the automatic limit, or after a failure). */
  download: [attachmentId: string]
  /** Show the file in the viewer. */
  view: [file: ChatPeerAttachment]
  openSystem: [attachmentId: string]
  save: [file: ChatPeerAttachment]
}>()

const { t, te } = useI18n()
const { errString } = useErrorString()
const { capabilities } = useDeviceCapabilities()

const control = computed(() =>
  props.own && props.file.state === null ? null : textControl(props.file),
)
/** The type icon: the media type tells images, audio, video and text apart; the rest is a file. */
const icon = computed(() => {
  const mime = props.file.mime ?? ''
  if (mime.startsWith('image/')) return 'lucide:image'
  if (mime.startsWith('video/')) return 'lucide:film'
  if (mime.startsWith('audio/')) return 'lucide:music'
  if (mime.startsWith('text/') || mime === 'application/pdf')
    return 'lucide:file-text'
  return 'lucide:file'
})
/** The copy is on this device. */
const here = computed(
  () => control.value === null || control.value.kind === 'toggle',
)
/** The viewer shows this kind of file (the info view alone would add nothing). */
const viewable = computed(() => here.value && viewerKind(name.value) !== 'info')
/** A phone has no app to hand a copy to yet (spec 044 R11). */
const openable = computed(
  () =>
    here.value &&
    capabilities.value !== null &&
    capabilities.value.platform !== 'android' &&
    capabilities.value.platform !== 'ios',
)
const name = computed(() => props.file.name ?? t('chat.file.unnamed'))
const error = computed(() => {
  const reason = props.file.error
  if (!reason) return t('chat.file.failed')
  if (reason.startsWith('chat.file.'))
    return te(reason) ? t(reason) : t('chat.file.failed')
  return errString(reason)
})
</script>

<template>
  <div
    class="flex min-w-0 items-center gap-2 rounded-lg bg-background/10 px-2 py-1.5"
    data-testid="chat-attachment-bubble"
    :data-state="file.state ?? undefined"
  >
    <Icon :name="icon" class="h-6 w-6 shrink-0" aria-hidden="true" />
    <div class="min-w-0 flex-1">
      <p
        class="truncate text-sm"
        :title="name"
        data-testid="chat-attachment-name"
      >
        {{ name }}
      </p>
      <p
        v-if="control?.kind === 'loading'"
        class="text-xs opacity-80"
        data-testid="chat-attachment-loading"
      >
        {{
          control.percent === null
            ? t('chat.file.loading')
            : t('chat.file.loadingPercent', { percent: control.percent })
        }}
      </p>
      <p
        v-else-if="control?.kind === 'gone'"
        class="text-xs opacity-80"
        data-testid="chat-attachment-gone"
      >
        {{ t('chat.file.gone') }}
      </p>
      <p
        v-else-if="control?.kind === 'retry'"
        class="text-xs opacity-80"
        data-testid="chat-attachment-error"
      >
        {{ error }}
      </p>
      <p v-else class="text-xs opacity-80">
        {{ formatFileSize(file.size) }}
      </p>
    </div>
    <UiButton
      v-if="control?.kind === 'download' || control?.kind === 'retry'"
      type="button"
      size="sm"
      variant="secondary"
      data-testid="chat-attachment-download"
      @click="emit('download', file.id)"
    >
      {{
        control.kind === 'retry'
          ? t('chat.file.retry')
          : t('chat.file.download', { size: formatFileSize(file.size) })
      }}
    </UiButton>
    <UiButtonIcon
      v-if="viewable"
      :label="t('chat.file.view')"
      data-testid="chat-attachment-view"
      @click="emit('view', file)"
      ><Icon name="lucide:eye" class="size-4"
    /></UiButtonIcon>
    <UiButtonSave
      v-if="here"
      :label="t('chat.file.save')"
      data-testid="chat-attachment-save"
      @click="emit('save', file)"
    />
    <UiButtonOpenExternal
      v-if="openable"
      :label="t('chat.file.openSystem')"
      data-testid="chat-attachment-open-system"
      @click="emit('openSystem', file.id)"
    />
  </div>
</template>
