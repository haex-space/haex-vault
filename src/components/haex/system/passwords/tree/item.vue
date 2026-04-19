<template>
  <div>
    <div
      :class="[
        'flex items-center gap-2 pe-2 rounded-md cursor-pointer transition-colors min-h-12',
        isActive ? 'bg-elevated font-medium' : 'hover:bg-elevated/70',
        isDragging && 'opacity-40',
        isDropTarget && 'ring-2 ring-primary ring-offset-1 ring-offset-transparent',
      ]"
      :style="{ paddingInlineStart: `${level * 20 + 4}px` }"
      draggable="true"
      @click="onSelect"
      @dragstart="onDragStart"
      @dragend="onDragEnd"
      @dragover.prevent="onDragOver"
      @dragleave="onDragLeave"
      @drop.prevent="onDropAsync"
      @contextmenu.prevent="onContextMenu"
    >
      <button
        type="button"
        class="size-8 shrink-0 flex items-center justify-center rounded hover:bg-muted/40"
        :class="{ invisible: !hasChildren }"
        @click.stop="toggleExpanded(group.id)"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="size-4 transition-transform"
          :class="{ 'rotate-90': expanded }"
        />
      </button>

      <div
        class="size-9 shrink-0 flex items-center justify-center rounded-md overflow-hidden"
        :style="folderBackgroundStyle"
      >
        <UIcon
          :name="folderIconName"
          class="size-5"
          :style="folderGlyphStyle"
        />
      </div>

      <span class="flex-1 text-[15px] truncate py-2">{{ group.name || t('untitled') }}</span>

      <UDropdownMenu
        :items="menuItems"
        :content="{ align: 'end' }"
      >
        <UButton
          icon="i-lucide-more-horizontal"
          color="neutral"
          variant="ghost"
          class="shrink-0"
          @click.stop
        />
      </UDropdownMenu>
    </div>

    <div
      v-if="expanded && hasChildren"
      class="space-y-0.5"
    >
      <HaexSystemPasswordsTreeItem
        v-for="child in children"
        :key="child.id"
        :group="child"
        :level="level + 1"
        @edit="$emit('edit', $event)"
        @create-child="$emit('createChild', $event)"
        @delete="$emit('delete', $event)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import type { DropdownMenuItem } from '@nuxt/ui'
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const props = defineProps<{
  group: SelectHaexPasswordsGroups
  level: number
}>()

const emit = defineEmits<{
  edit: [group: SelectHaexPasswordsGroups]
  createChild: [parentId: string]
  delete: [group: SelectHaexPasswordsGroups]
}>()

const { t } = useI18n()

const groupsStore = usePasswordsGroupsStore()
const {
  selectedGroupId,
  childrenByParent,
} = storeToRefs(groupsStore)
const { selectGroup, setItemGroupAsync, moveGroupAsync, descendantIdSet } =
  groupsStore

const { isExpanded, toggleExpanded, setExpanded, expandAncestors } =
  useTreeExpanded()

const children = computed(
  () => childrenByParent.value.get(props.group.id) ?? [],
)
const hasChildren = computed(() => children.value.length > 0)
const expanded = computed(() => isExpanded(props.group.id))
const isActive = computed(() => selectedGroupId.value === props.group.id)

const folderIconName = computed(() => {
  if (props.group.icon) return props.group.icon
  return expanded.value && hasChildren.value
    ? 'i-lucide-folder-open'
    : 'i-lucide-folder'
})

const folderBackgroundStyle = computed(() =>
  props.group.color ? { backgroundColor: props.group.color } : undefined,
)

const folderGlyphStyle = computed(() => {
  if (!props.group.color) return { color: 'rgb(var(--ui-primary))' }
  const hex = props.group.color.replace('#', '')
  const r = parseInt(hex.slice(0, 2), 16)
  const g = parseInt(hex.slice(2, 4), 16)
  const b = parseInt(hex.slice(4, 6), 16)
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
  return { color: luminance > 0.6 ? '#111827' : '#ffffff' }
})

const onSelect = () => {
  selectGroup(props.group.id)
}

const menuItems = computed<DropdownMenuItem[][]>(() => [
  [
    {
      label: t('edit'),
      icon: 'i-lucide-pencil',
      onSelect: () => emit('edit', props.group),
    },
    {
      label: t('newSubfolder'),
      icon: 'i-lucide-folder-plus',
      onSelect: () => {
        setExpanded(props.group.id, true)
        emit('createChild', props.group.id)
      },
    },
  ],
  [
    {
      label: t('delete'),
      icon: 'i-lucide-trash-2',
      color: 'error' as const,
      onSelect: () => emit('delete', props.group),
    },
  ],
])

const onContextMenu = () => {
  emit('edit', props.group)
}

const isDragging = ref(false)
const isDropTarget = ref(false)

const onDragStart = (event: DragEvent) => {
  if (!event.dataTransfer) return
  isDragging.value = true
  event.dataTransfer.effectAllowed = 'move'
  event.dataTransfer.setData('application/x-haex-group', props.group.id)
}

const onDragEnd = () => {
  isDragging.value = false
}

const onDragOver = (event: DragEvent) => {
  if (!event.dataTransfer) return
  const types = event.dataTransfer.types
  const isItemDrag = types.includes('application/x-haex-item')
  const isGroupDrag = types.includes('application/x-haex-group')
  if (!isItemDrag && !isGroupDrag) return
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
    await setItemGroupAsync(itemId, props.group.id)
    setExpanded(props.group.id, true)
    return
  }

  const draggedGroupId = event.dataTransfer.getData('application/x-haex-group')
  if (!draggedGroupId || draggedGroupId === props.group.id) return
  if (descendantIdSet(draggedGroupId).has(props.group.id)) return

  await moveGroupAsync(draggedGroupId, props.group.id)
  setExpanded(props.group.id, true)
}

watch(
  selectedGroupId,
  (next) => {
    if (!next) return
    if (next === props.group.id || descendantIdSet(props.group.id).has(next)) {
      setExpanded(props.group.id, true)
    }
  },
  { immediate: true },
)
</script>

<i18n lang="yaml">
de:
  untitled: (ohne Namen)
  edit: Bearbeiten
  newSubfolder: Unterordner anlegen
  delete: Löschen
en:
  untitled: (unnamed)
  edit: Edit
  newSubfolder: New subfolder
  delete: Delete
</i18n>
