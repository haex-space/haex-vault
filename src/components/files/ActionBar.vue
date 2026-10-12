<script setup lang="ts">
/**
 * The bar below the folder while entries are selected or something waits in holzi's clipboard
 * (spec 044 FR-017, FR-018): the same commands as the context menu, reachable on touch without a
 * long press.
 */
import type { FilesCommand } from '~/lib/files/menus'

defineProps<{
  selected: number
  /** Something in the clipboard may go into the open folder. */
  canPaste: boolean
  readOnly: boolean
}>()
const emit = defineEmits<{ run: [command: FilesCommand]; clear: [] }>()

const { t } = useI18n()
</script>

<template>
  <div
    class="flex h-11 shrink-0 items-center gap-1 border-t bg-background px-2 text-sm"
    data-testid="files-action-bar"
  >
    <template v-if="selected">
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('files.selection.clear')"
        :tooltip="t('files.selection.clear')"
        data-testid="files-selection-clear"
        @click="emit('clear')"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
      <span class="mr-auto" data-testid="files-selection-count">{{
        t('files.selection.count', { count: selected }, selected)
      }}</span>
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('files.actions.copy')"
        :tooltip="t('files.actions.copy')"
        data-testid="files-action-copy"
        @click="emit('run', 'copy')"
      >
        <Icon name="lucide:copy" class="size-4" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="readOnly"
        :aria-label="t('files.actions.cut')"
        :tooltip="t('files.actions.cut')"
        data-testid="files-action-cut"
        @click="emit('run', 'cut')"
      >
        <Icon name="lucide:scissors" class="size-4" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="readOnly || selected !== 1"
        :aria-label="t('files.actions.rename')"
        :tooltip="t('files.actions.rename')"
        data-testid="files-action-rename"
        @click="emit('run', 'rename')"
      >
        <Icon name="lucide:pencil" class="size-4" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="readOnly"
        :aria-label="t('files.actions.delete')"
        :tooltip="t('files.actions.delete')"
        data-testid="files-action-delete"
        @click="emit('run', 'delete')"
      >
        <Icon name="lucide:trash-2" class="size-4" />
      </UiButton>
    </template>
    <span v-else class="mr-auto text-muted-foreground">{{
      t('files.clipboard.waiting')
    }}</span>
    <UiButton
      v-if="canPaste"
      variant="outline"
      size="sm"
      :disabled="readOnly"
      data-testid="files-action-paste"
      @click="emit('run', 'paste')"
    >
      <Icon name="lucide:clipboard-paste" class="size-4" />
      {{ t('files.actions.paste') }}
    </UiButton>
  </div>
</template>
