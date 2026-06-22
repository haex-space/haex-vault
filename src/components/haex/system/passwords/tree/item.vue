<template>
  <div>
    <UContextMenu :items="menuItems">
      <div
        ref="rowRef"
        :class="[
          'flex items-center gap-2 pe-2 rounded-md cursor-pointer transition-colors min-h-12',
          isActive ? 'bg-elevated font-medium' : 'hover:bg-elevated/70',
          isDragging && 'opacity-40',
          isCut && 'opacity-50 grayscale',
          (isDropTarget || isMultiSelected) && 'ring-2 ring-primary ring-offset-1 ring-offset-transparent',
        ]"
        :style="{ paddingInlineStart: `${level * 20 + 4}px` }"
        draggable="true"
        @click="onRowClick"
        @dragstart="onDragStart"
        @dragend="onDragEnd"
        @dragover.prevent="onDragOver"
        @dragleave="onDragLeave"
        @drop.prevent="onDropAsync"
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
      </div>
    </UContextMenu>

    <div
      v-if="expanded && hasChildren"
      class="space-y-1.5 mt-1.5"
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
import type { ContextMenuItem } from '@nuxt/ui'
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
const toast = useToast()

const groupsStore = usePasswordsGroupsStore()
const {
  selectedGroupId,
  childrenByParent,
} = storeToRefs(groupsStore)
const { descendantIdSet } = groupsStore
const { selectGroup } = usePasswordsNavigation()

const isInTrash = computed(() => groupsStore.isGroupInTrash(props.group.id))

const { isExpanded, toggleExpanded, setExpanded } = useTreeExpanded()

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

const { backgroundStyle: folderBackgroundStyle, glyphStyle: folderGlyphStyle }
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
} = usePasswordsGroupRow(() => props.group, {
  // After a successful drop, expand this group so the user immediately sees
  // the entry/subfolder they just moved in.
  onAfterDrop: () => setExpanded(props.group.id, true),
})

const selection = usePasswordsSelectionStore()

const onRowClick = (event: MouseEvent) => {
  if (shouldSuppressClick()) return
  if (event.ctrlKey || event.metaKey) {
    event.preventDefault()
    event.stopPropagation()
    selection.toggle(props.group.id)
    return
  }
  if (isSelectionMode.value) {
    // While in selection mode, plain clicks in the tree also toggle so the
    // user can tick off multiple sibling groups without reaching for Ctrl.
    selection.toggle(props.group.id)
    return
  }
  selectGroup(props.group.id)
}

const menuItems = computed<ContextMenuItem[][]>(() => {
  if (isInTrash.value) {
    return [
      [
        {
          label: t('restore'),
          icon: 'i-lucide-undo-2',
          onSelect: async () => {
            try {
              await groupsStore.restoreGroupAsync(props.group.id)
              toast.add({ title: t('toast.restored'), color: 'success' })
            } catch (error) {
              console.error('[TreeItem] Restore failed:', error)
            }
          },
        },
      ],
      [
        {
          label: t('deletePermanently'),
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
        icon: 'i-lucide-trash',
        color: 'error' as const,
        onSelect: () => emit('delete', props.group),
      },
    ],
  ]
})

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
  delete: In Papierkorb
  restore: Wiederherstellen
  deletePermanently: Endgültig löschen
  toast:
    restored: Wiederhergestellt
en:
  untitled: (unnamed)
  edit: Edit
  newSubfolder: New subfolder
  delete: Move to trash
  restore: Restore
  deletePermanently: Delete permanently
  toast:
    restored: Restored
</i18n>
