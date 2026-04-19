<template>
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
        <div class="flex items-center gap-1 text-muted">
          <UDropdownMenu
            :items="menuItems"
            :content="{ align: 'end' }"
          >
            <UButton
              icon="i-lucide-more-horizontal"
              color="neutral"
              variant="ghost"
              @click.stop
            />
          </UDropdownMenu>
          <UIcon
            name="i-lucide-chevron-right"
            class="size-4"
          />
        </div>
      </template>
    </UiListItem>
  </div>
</template>

<script setup lang="ts">
import type { DropdownMenuItem } from '@nuxt/ui'
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const props = defineProps<{
  group: SelectHaexPasswordsGroups
}>()

const emit = defineEmits<{
  edit: [group: SelectHaexPasswordsGroups]
  delete: [group: SelectHaexPasswordsGroups]
}>()

const { t } = useI18n()
const groupsStore = usePasswordsGroupsStore()
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

const iconBackgroundStyle = computed(() =>
  props.group.color ? { backgroundColor: props.group.color } : undefined,
)

const iconGlyphStyle = computed(() => {
  if (!props.group.color) return { color: 'rgb(var(--ui-primary))' }
  const hex = props.group.color.replace('#', '')
  if (hex.length !== 6) return undefined
  const r = parseInt(hex.slice(0, 2), 16)
  const g = parseInt(hex.slice(2, 4), 16)
  const b = parseInt(hex.slice(4, 6), 16)
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
  return { color: luminance > 0.6 ? '#111827' : '#ffffff' }
})

const selection = usePasswordsSelectionStore()
const { isSelectionMode } = storeToRefs(selection)

const isMultiSelected = computed(() => selection.isSelected(props.group.id))
const isCut = computed(() => selection.isCut(props.group.id))

const orderedIds = inject<Ref<string[]>>(
  'passwordsList:orderedIds',
  ref<string[]>([]),
)

const rowRef = useTemplateRef<HTMLElement>('rowRef')
const { shouldSuppressClick } = useLongPressSelection(rowRef, () => {
  if (!isSelectionMode.value) {
    selection.enterSelectionWith(props.group.id)
  } else {
    selection.toggle(props.group.id)
  }
})

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
  groupsStore.selectGroup(props.group.id)
}

const onCheckboxClick = (event: MouseEvent) => {
  if (event.shiftKey) {
    selection.selectRange(props.group.id, orderedIds.value)
    return
  }
  selection.toggle(props.group.id)
}

const menuItems = computed<DropdownMenuItem[][]>(() => [
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
      icon: 'i-lucide-trash-2',
      color: 'error' as const,
      onSelect: () => emit('delete', props.group),
    },
  ],
])

const isDragging = ref(false)
const isDropTarget = ref(false)

const onDragStart = (event: DragEvent) => {
  if (!event.dataTransfer) return
  event.dataTransfer.effectAllowed = 'move'
  event.dataTransfer.setData('application/x-haex-group', props.group.id)
  isDragging.value = true
}

const onDragEnd = () => {
  isDragging.value = false
}

const onDragOver = (event: DragEvent) => {
  if (!event.dataTransfer) return
  const types = event.dataTransfer.types
  if (
    !types.includes('application/x-haex-item') &&
    !types.includes('application/x-haex-group')
  )
    return
  event.dataTransfer.dropEffect = 'move'
  isDropTarget.value = true
}

const onDragLeave = () => {
  isDropTarget.value = false
}

const onDropAsync = async (event: DragEvent) => {
  isDropTarget.value = false
  if (!event.dataTransfer) return

  const itemId = event.dataTransfer.getData('application/x-haex-item')
  if (itemId) {
    await groupsStore.setItemGroupAsync(itemId, props.group.id)
    return
  }

  const draggedGroupId = event.dataTransfer.getData('application/x-haex-group')
  if (!draggedGroupId || draggedGroupId === props.group.id) return
  if (groupsStore.descendantIdSet(draggedGroupId).has(props.group.id)) return

  await groupsStore.moveGroupAsync(draggedGroupId, props.group.id)
}
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
    delete: Löschen
en:
  untitled: (unnamed)
  subfolders: "{count} folder | {count} folders"
  items: "{count} entry | {count} entries"
  select: Select
  deselect: Deselect
  menu:
    edit: Edit
    delete: Delete
</i18n>
