<script setup lang="ts">
/**
 * The right-click menu of the open folder (a long press on touch; spec 044 T050): one menu around
 * the whole list, its entries from `lib/files/menus.ts` for whatever the pointer was over, each with
 * its keyboard shortcut.
 */
import type { FilesCommand, FilesMenuEntry } from '~/lib/files/menus'

defineProps<{ entries: readonly FilesMenuEntry[] }>()
const emit = defineEmits<{ run: [command: FilesCommand] }>()

const { t } = useI18n()
const { shortcut } = useShortcutText()

/** Closing gives the focus back to the list, unless the command opened a dialog (rename, a new
 * folder, the viewer), which took it already. */
function onCloseAutoFocus(event: Event) {
  if (document.querySelector('[role="dialog"], [role="alertdialog"]'))
    event.preventDefault()
}
</script>

<template>
  <ShadcnContextMenu>
    <ShadcnContextMenuTrigger as-child>
      <slot />
    </ShadcnContextMenuTrigger>
    <ShadcnContextMenuContent
      class="min-w-48"
      data-testid="files-context-menu"
      @close-auto-focus="onCloseAutoFocus"
    >
      <template v-for="(entry, index) in entries" :key="index">
        <ShadcnContextMenuSeparator v-if="'separator' in entry" />
        <ShadcnContextMenuItem
          v-else
          :disabled="entry.disabled"
          :data-testid="`files-menu-${entry.id}`"
          @select="emit('run', entry.id)"
        >
          {{ t(entry.labelKey) }}
          <ShadcnContextMenuShortcut v-if="entry.shortcut">
            {{ shortcut(entry.shortcut) }}
          </ShadcnContextMenuShortcut>
        </ShadcnContextMenuItem>
      </template>
    </ShadcnContextMenuContent>
  </ShadcnContextMenu>
</template>
