<template>
  <div
    v-if="isSelectionMode || hasClipboard"
    class="@container/toolbar flex items-center gap-2 px-3 py-2 bg-primary text-inverted border-b border-primary/40 min-h-12"
  >
    <UButton
      icon="i-lucide-x"
      variant="ghost"
      color="neutral"
      class="shrink-0 text-inverted hover:bg-white/10"
      :aria-label="t('close')"
      @click="onClose"
    />

    <!-- Select-all checkbox — visible in selection mode only -->
    <button
      v-if="isSelectionMode"
      type="button"
      :class="[
        'shrink-0 size-6 rounded border flex items-center justify-center transition-colors',
        allSelected
          ? 'bg-white/30 border-white/60'
          : 'border-white/40 hover:border-white/80',
      ]"
      :aria-label="allSelected ? t('deselectAll') : t('selectAll')"
      @click="onToggleAll"
    >
      <UIcon
        v-if="allSelected"
        name="i-lucide-check"
        class="size-4"
      />
      <UIcon
        v-else-if="someSelected"
        name="i-lucide-minus"
        class="size-4"
      />
    </button>

    <span
      v-if="isSelectionMode"
      class="text-sm font-medium truncate min-w-0 flex-1"
    >
      {{ t('selectedCount', { count: selectedCount }) }}
    </span>
    <span
      v-else
      class="text-sm font-medium truncate min-w-0 flex-1"
    >
      {{
        clipboardMode === 'cut'
          ? t('clipboardCut', { count: clipboardEntries.length })
          : t('clipboardCopy', { count: clipboardEntries.length })
      }}
    </span>

    <!-- Action buttons — shown inline on wide containers -->
    <div class="hidden @[22rem]/toolbar:flex items-center gap-1 shrink-0">
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

    <!-- Overflow dropdown — shown on narrow containers where inline buttons don't fit -->
    <div class="flex @[22rem]/toolbar:hidden shrink-0">
      <UDropdownMenu
        :items="overflowMenuItems"
        :content="{ align: 'end' }"
      >
        <UButton
          icon="i-lucide-more-horizontal"
          variant="ghost"
          color="neutral"
          class="text-inverted hover:bg-white/10"
          :aria-label="t('more')"
        />
      </UDropdownMenu>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { DropdownMenuItem } from '@nuxt/ui'

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
  selectedIds,
} = storeToRefs(selection)

const passwordsStore = usePasswordsStore()
const nav = usePasswordsNavigation()

const emit = defineEmits<{
  tag: []
  delete: []
  paste: []
  editGroup: [groupId: string]
}>()

// Select-all state — driven by the full visible list provided from the parent.
const visibleOrderedIds = inject<Ref<string[]>>('passwords:visibleOrderedIds', ref([]))

const allSelected = computed(
  () =>
    visibleOrderedIds.value.length > 0 &&
    visibleOrderedIds.value.every((id) => selectedIds.value.has(id)),
)
const someSelected = computed(
  () =>
    !allSelected.value &&
    visibleOrderedIds.value.some((id) => selectedIds.value.has(id)),
)

const onToggleAll = () => {
  if (allSelected.value) {
    selection.clear()
  } else {
    selection.selectAll(visibleOrderedIds.value)
  }
}

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

// Builds the overflow dropdown items matching the inline button set.
const overflowMenuItems = computed<DropdownMenuItem[][]>(() => {
  if (!isSelectionMode.value) {
    return [[
      { label: t('paste'), icon: 'i-lucide-clipboard-paste', onSelect: () => emit('paste') },
      { label: t('cancelClipboard'), icon: 'i-lucide-x-circle', onSelect: () => selection.clearClipboard() },
    ]]
  }
  const items: DropdownMenuItem[] = []
  if (selectedCount.value === 1) {
    items.push({ label: t('edit'), icon: 'i-lucide-pencil', onSelect: onEdit })
  }
  items.push(
    { label: t('copy'), icon: 'i-lucide-copy', onSelect: () => selection.copyToClipboard() },
    { label: t('cut'), icon: 'i-lucide-scissors', onSelect: () => selection.cutToClipboard() },
  )
  if (isItemsOnly.value) {
    items.push({ label: t('tag'), icon: 'i-lucide-tag', onSelect: () => emit('tag') })
  }
  items.push({ label: t('delete'), icon: 'i-lucide-trash-2', color: 'error' as const, onSelect: () => emit('delete') })
  return [items]
})
</script>

<i18n lang="yaml">
de:
  close: Schließen
  selectAll: Alle auswählen
  deselectAll: Alle abwählen
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
  more: Mehr
en:
  close: Close
  selectAll: Select all
  deselectAll: Deselect all
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
  more: More
</i18n>
