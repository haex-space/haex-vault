<script setup lang="ts">
/**
 * The files picked for the message being written in a contact conversation (spec 072 US3), as chips
 * in the first row of the input box. Each can be taken off again until the message is sent; while
 * the files go up to the Blossom-Server the chips stay and cannot be changed (FR-037).
 */
export interface PeerFile {
  /** The choice of the open dialog: a path, or a `content://` address on Android. */
  path: string
  name: string
  /** The bytes the files of one message add up with (FR-034). */
  size: number
}

defineProps<{
  files: PeerFile[]
  /** The message is being sent: the files are going up. */
  sending: boolean
}>()

const emit = defineEmits<{ remove: [path: string] }>()

const { t } = useI18n()
</script>

<template>
  <div
    class="flex min-w-0 flex-wrap items-center gap-1.5 border-b border-border px-3 pb-3 pt-3"
    data-testid="chat-peer-files"
  >
    <div
      v-for="file in files"
      :key="file.path"
      class="flex max-w-full shrink-0 items-center gap-1 rounded-lg border border-border/70 bg-muted/20 py-1 pl-2 pr-1 text-xs"
      :title="file.name"
      data-testid="chat-peer-file"
    >
      <Icon
        name="lucide:file"
        class="h-3.5 w-3.5 shrink-0"
        aria-hidden="true"
      />
      <span class="max-w-[12rem] truncate">{{ file.name }}</span>
      <Icon
        v-if="sending"
        name="lucide:loader-circle"
        class="h-3.5 w-3.5 animate-spin"
        :aria-label="t('chat.peer.sendingFile')"
      />
      <button
        v-else
        type="button"
        class="flex h-5 w-5 items-center justify-center rounded outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
        :aria-label="t('chat.peer.removeFile', { name: file.name })"
        data-testid="chat-peer-file-remove"
        @click="emit('remove', file.path)"
      >
        <Icon name="lucide:x" class="h-3 w-3" aria-hidden="true" />
      </button>
    </div>
  </div>
</template>
