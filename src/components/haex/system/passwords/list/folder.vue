<template>
  <UContextMenu :items="menuItems">
    <div
      ref="rowRef"
      :class="[
        isDragging && 'opacity-40',
        isCut && 'opacity-50 grayscale',
        (isDropTarget || isMultiSelected) && 'ring-2 ring-primary rounded-lg',
      ]"
      draggable="true"
      @dragstart="onDragStart"
      @dragend="onDragEnd"
      @dragover.prevent="onDragOver"
      @dragleave="onDragLeave"
      @drop.prevent="onDropAsync"
    >
      <UiListItem
        class="cursor-pointer"
        @click="onRowClick"
      >
        <div class="flex items-center gap-3 min-h-14">
          <button
            v-if="isSelectionMode"
            type="button"
            :class="[
              'shrink-0 size-6 rounded border flex items-center justify-center transition-colors',
              isMultiSelected
                ? 'bg-primary border-primary text-inverted'
                : 'border-default hover:border-primary',
            ]"
            :aria-label="isMultiSelected ? t('deselect') : t('select')"
            @click.stop="onCheckboxClick"
          >
            <UIcon
              v-if="isMultiSelected"
              name="i-lucide-check"
              class="size-4"
            />
          </button>
          <div
            class="shrink-0 size-10 rounded-md flex items-center justify-center bg-elevated overflow-hidden"
            :style="iconBackgroundStyle"
          >
            <UIcon
              :name="folderIconName"
              class="size-6"
              :style="iconGlyphStyle"
            />
          </div>

          <div class="flex-1 min-w-0">
            <p class="font-medium truncate">
              {{ group.name || t('untitled') }}
            </p>

            <div
              v-if="countDescription"
              class="mt-0.5 flex items-center gap-3 text-xs text-muted"
            >
              <span>{{ countDescription }}</span>
            </div>
          </div>
        </div>

        <template #actions>
          <UIcon
            name="i-lucide-chevron-right"
            class="size-4 text-muted"
          />
        </template>
      </UiListItem>
    </div>
  </UContextMenu>
</template>

<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui'
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const props = defineProps<{
  group: SelectHaexPasswordsGroups
}>()

const emit = defineEmits<{
  edit: [group: SelectHaexPasswordsGroups]
  delete: [group: SelectHaexPasswordsGroups]
}>()

const toast = useToast()
const { t } = useI18n()
const groupsStore = usePasswordsGroupsStore()
const { selectGroup } = usePasswordsNavigation()
const isInTrash = computed(() => groupsStore.isGroupInTrash(props.group.id))
const {
  childrenByParent,
  itemCountByGroupId,
} = storeToRefs(groupsStore)

const childFolders = computed(
  () => childrenByParent.value.get(props.group.id) ?? [],
)

const directItemCount = computed(
  () => itemCountByGroupId.value.get(props.group.id) ?? 0,
)

const countDescription = computed(() => {
  const parts: string[] = []
  if (childFolders.value.length > 0) {
    parts.push(t('subfolders', { count: childFolders.value.length }))
  }
  if (directItemCount.value > 0) {
    parts.push(t('items', { count: directItemCount.value }))
  }
  return parts.join(' · ')
})

const folderIconName = computed(
  () => props.group.icon || 'i-lucide-folder',
)

const { backgroundStyle: iconBackgroundStyle, glyphStyle: iconGlyphStyle }
  = usePasswordsGroupStyles(() => props.group)

const {
  rowRef,
  isSelectionMode,
  isMultiSelected,
  isCut,
  shouldSuppressClick,
  isDragging,
  isDropTarget,
  onDragStart,
  onDragEnd,
  onDragOver,
  onDragLeave,
  onDropAsync,
} = usePasswordsGroupRow(() => props.group)

const selection = usePasswordsSelectionStore()

const orderedIds = inject<Ref<string[]>>(
  'passwordsList:orderedIds',
  ref<string[]>([]),
)

const onRowClick = (event: MouseEvent) => {
  if (shouldSuppressClick()) return
  if (event.shiftKey) {
    event.preventDefault()
    selection.selectRange(props.group.id, orderedIds.value)
    return
  }
  if (event.ctrlKey || event.metaKey) {
    event.preventDefault()
    selection.toggle(props.group.id)
    return
  }
  if (isSelectionMode.value) {
    selection.toggle(props.group.id)
    return
  }
  selectGroup(props.group.id)
}

const onCheckboxClick = (event: MouseEvent) => {
  if (event.shiftKey) {
    selection.selectRange(props.group.id, orderedIds.value)
    return
  }
  selection.toggle(props.group.id)
}

const menuItems = computed<ContextMenuItem[][]>(() => {
  if (isInTrash.value) {
    return [
      [
        {
          label: t('menu.restore'),
          icon: 'i-lucide-undo-2',
          onSelect: async () => {
            try {
              await groupsStore.restoreGroupAsync(props.group.id)
              toast.add({ title: t('toast.restored'), color: 'success' })
            } catch (error) {
              console.error('[ListFolder] Restore failed:', error)
            }
          },
        },
      ],
      [
        {
          label: t('menu.deletePermanently'),
          icon: 'i-lucide-trash-2',
          color: 'error' as const,
          onSelect: () => emit('delete', props.group),
        },
      ],
    ]
  }
  return [
    [
      {
        label: t('menu.edit'),
        icon: 'i-lucide-pencil',
        onSelect: () => emit('edit', props.group),
      },
    ],
    [
      {
        label: t('menu.delete'),
        icon: 'i-lucide-trash',
        color: 'error' as const,
        onSelect: () => emit('delete', props.group),
      },
    ],
  ]
})

</script>

<i18n lang="yaml">
de:
  untitled: (ohne Namen)
  subfolders: "{count} Ordner | {count} Ordner"
  items: "{count} Eintrag | {count} Einträge"
  select: Auswählen
  deselect: Abwählen
  menu:
    edit: Bearbeiten
    delete: In Papierkorb
    restore: Wiederherstellen
    deletePermanently: Endgültig löschen
  toast:
    restored: Wiederhergestellt
en:
  untitled: (unnamed)
  subfolders: "{count} folder | {count} folders"
  items: "{count} entry | {count} entries"
  select: Select
  deselect: Deselect
  menu:
    edit: Edit
    delete: Move to trash
    restore: Restore
    deletePermanently: Delete permanently
  toast:
    restored: Restored
</i18n>
