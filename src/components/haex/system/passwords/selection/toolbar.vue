<template>
  <div
    v-if="isSelectionMode || hasClipboard"
    class="flex items-center justify-between gap-2 px-3 py-2 bg-primary text-inverted border-b border-primary/40 min-h-12"
  >
    <div class="flex items-center gap-2 min-w-0">
      <UButton
        icon="i-lucide-x"
        variant="ghost"
        color="neutral"
        class="shrink-0 text-inverted hover:bg-white/10"
        :aria-label="t('close')"
        @click="onClose"
      />
      <span
        v-if="isSelectionMode"
        class="text-sm font-medium truncate"
      >
        {{ t('selectedCount', { count: selectedCount }) }}
      </span>
      <span
        v-else
        class="text-sm font-medium truncate"
      >
        {{
          clipboardMode === 'cut'
            ? t('clipboardCut', { count: clipboardEntries.length })
            : t('clipboardCopy', { count: clipboardEntries.length })
        }}
      </span>
    </div>

    <div class="flex items-center gap-1 shrink-0">
      <template v-if="isSelectionMode">
        <UButton
          v-if="selectedCount === 1"
          icon="i-lucide-pencil"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('edit')"
          @click="onEdit"
        />
        <UButton
          icon="i-lucide-copy"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('copy')"
          @click="selection.copyToClipboard()"
        />
        <UButton
          icon="i-lucide-scissors"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('cut')"
          @click="selection.cutToClipboard()"
        />
        <UButton
          v-if="isItemsOnly"
          icon="i-lucide-tag"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('tag')"
          @click="emit('tag')"
        />
        <UButton
          icon="i-lucide-trash-2"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('delete')"
          @click="emit('delete')"
        />
      </template>
      <template v-else>
        <UButton
          icon="i-lucide-clipboard-paste"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('paste')"
          @click="emit('paste')"
        />
        <UButton
          icon="i-lucide-x-circle"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('cancelClipboard')"
          @click="selection.clearClipboard()"
        />
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()

const selection = usePasswordsSelectionStore()
const {
  isSelectionMode,
  selectedCount,
  hasClipboard,
  clipboardMode,
  clipboardEntries,
  isItemsOnly,
  selectedEntries,
} = storeToRefs(selection)

const passwordsStore = usePasswordsStore()
const nav = usePasswordsNavigation()

const emit = defineEmits<{
  tag: []
  delete: []
  paste: []
  editGroup: [groupId: string]
}>()

const onClose = () => {
  if (isSelectionMode.value) {
    selection.clear()
  } else {
    selection.clearClipboard()
  }
}

const onEdit = () => {
  const only = selectedEntries.value[0]
  if (!only) return
  if (only.type === 'item') {
    passwordsStore.openItem(only.id)
    nav.startEdit()
    selection.clear()
  } else {
    emit('editGroup', only.id)
    selection.clear()
  }
}
</script>

<i18n lang="yaml">
de:
  close: Schließen
  selectedCount: "{count} ausgewählt"
  clipboardCut: "{count} in Zwischenablage (Ausschneiden)"
  clipboardCopy: "{count} in Zwischenablage (Kopieren)"
  edit: Bearbeiten
  copy: Kopieren
  cut: Ausschneiden
  tag: Tag hinzufügen/entfernen
  delete: Löschen
  paste: Einfügen
  cancelClipboard: Zwischenablage leeren
en:
  close: Close
  selectedCount: "{count} selected"
  clipboardCut: "{count} on clipboard (cut)"
  clipboardCopy: "{count} on clipboard (copy)"
  edit: Edit
  copy: Copy
  cut: Cut
  tag: Add/remove tag
  delete: Delete
  paste: Paste
  cancelClipboard: Clear clipboard
</i18n>
