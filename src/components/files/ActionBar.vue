<script setup lang="ts">
/**
 * The bar below the folder while entries are selected or something waits in haex-vault's clipboard
 * (spec 044 FR-017, FR-018): the same commands as the context menu, reachable on touch without a
 * long press.
 */
import type { FilesCommand } from '~/lib/files/menus'

defineProps<{
  selected: number
  /** Something in the clipboard may go into the open folder. */
  canPaste: boolean
  readOnly: boolean
  /** The tab shows two panels: buttons for copying and moving into the other one (spec 062). */
  split?: boolean
  /** The other panel's folder takes no changes. */
  readOnlyTarget?: boolean
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
      <UiButtonClose
        :label="t('files.selection.clear')"
        data-testid="files-selection-clear"
        @click="emit('clear')"
      />
      <span class="mr-auto" data-testid="files-selection-count">{{
        t('files.selection.count', { count: selected }, selected)
      }}</span>
      <UiButtonIcon
        :label="t('files.actions.copy')"
        data-testid="files-action-copy"
        @click="emit('run', 'copy')"
        ><Icon name="lucide:copy" class="size-4"
      /></UiButtonIcon>
      <UiButtonIcon
        :label="t('files.actions.cut')"
        :disabled="readOnly"
        data-testid="files-action-cut"
        @click="emit('run', 'cut')"
        ><Icon name="lucide:scissors" class="size-4"
      /></UiButtonIcon>
      <template v-if="split">
        <UiButtonIcon
          :label="`${t('files.actions.copyToOther')} (F5)`"
          :disabled="readOnlyTarget"
          data-testid="files-bar-copy-other"
          @click="emit('run', 'copyToOther')"
          ><Icon name="lucide:files" class="size-4"
        /></UiButtonIcon>
        <UiButtonIcon
          :label="`${t('files.actions.moveToOther')} (F6)`"
          :disabled="readOnly || readOnlyTarget"
          data-testid="files-bar-move-other"
          @click="emit('run', 'moveToOther')"
          ><Icon name="lucide:folder-input" class="size-4"
        /></UiButtonIcon>
      </template>
      <UiButtonEdit
        :label="t('files.actions.rename')"
        :disabled="readOnly || selected !== 1"
        data-testid="files-action-rename"
        @click="emit('run', 'rename')"
      />
      <UiButtonDelete
        :label="t('files.actions.delete')"
        :disabled="readOnly"
        data-testid="files-action-delete"
        @click="emit('run', 'delete')"
      />
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
