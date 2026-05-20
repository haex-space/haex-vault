<template>
  <div class="flex flex-col gap-1 h-full">
    <div class="flex items-center gap-1">
      <button
        type="button"
        class="flex-1 flex items-center gap-2 ps-3 pe-2 py-2 rounded-md text-[15px] transition-colors hover:bg-elevated min-h-12 min-w-0"
        :class="{ 'bg-elevated font-medium': selectedGroupId === null }"
        draggable="false"
        @click="selectGroup(null)"
        @dragover.prevent="onRootDragOver"
        @dragleave="onRootDragLeave"
        @drop.prevent="onRootDrop"
      >
        <UIcon
          name="i-lucide-key-round"
          class="size-5 shrink-0 text-primary"
        />
        <span class="truncate">{{ t('allPasswords') }}</span>
        <span
          v-if="isRootDropTarget"
          class="ms-auto text-xs text-primary"
        >
          {{ t('dropHere') }}
        </span>
      </button>
      <UButton
        icon="i-lucide-folder-plus"
        color="neutral"
        variant="ghost"
        class="shrink-0"
        :aria-label="t('newGroup')"
        @click="onCreateGroup"
      />
    </div>

    <div class="flex-1 overflow-y-auto space-y-1.5 px-0.5 py-1">
      <HaexSystemPasswordsTreeItem
        v-for="group in regularRootGroups"
        :key="group.id"
        :group="group"
        :level="0"
        @edit="onEditGroup"
        @create-child="onCreateChildGroup"
        @delete="onDeleteGroup"
      />
    </div>

    <div class="mt-3 pt-3 pb-1 border-t border-default">
      <UContextMenu
        v-model:open="trashMenuOpen"
        :items="trashContextMenuItems"
      >
        <button
          type="button"
          class="w-full flex items-center gap-2 ps-3 pe-2 py-2 rounded-md text-[15px] transition-colors hover:bg-elevated min-h-10"
          :class="{ 'bg-elevated font-medium': selectedGroupId === TRASH_GROUP_ID }"
          @click="selectGroup(TRASH_GROUP_ID)"
        >
          <UIcon
            name="i-lucide-trash-2"
            class="size-5 shrink-0 text-muted"
          />
          <span class="truncate text-muted">{{ t('trash') }}</span>
          <span
            v-if="trashItemCount > 0"
            class="ms-auto text-xs text-muted"
          >
            {{ trashItemCount }}
          </span>
        </button>
      </UContextMenu>
    </div>

    <HaexSystemPasswordsDialogEmptyTrash
      v-model:open="emptyTrashOpen"
      :item-count="trashItemCount"
      :group-count="trashGroupCount"
    />

    <HaexSystemPasswordsDialogGroupEditor
      v-model:open="editorOpen"
      :mode="editorMode"
      :group="editingGroup"
      :create-parent-id="createParentId"
    />

    <HaexSystemPasswordsDialogDeleteGroup
      v-model:open="deleteOpen"
      :group="deletingGroup"
      :final="deletingGroupIsFinal"
    />
  </div>
</template>

<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui'
import type { SelectHaexPasswordsGroups } from '~/database/schemas'
import { TRASH_GROUP_ID } from '~/stores/passwords/groups'

const { t } = useI18n()

const groupsStore = usePasswordsGroupsStore()
const { rootGroups, selectedGroupId, trashGroup, itemGroupMap } = storeToRefs(groupsStore)
const { setItemGroupAsync } = groupsStore
const { selectGroup } = usePasswordsNavigation()

const regularRootGroups = computed(
  () => rootGroups.value.filter((g) => g.id !== TRASH_GROUP_ID),
)

const trashItemCount = computed(() => {
  if (!trashGroup.value) return 0
  const trashDescendants = groupsStore.descendantIdSet(TRASH_GROUP_ID)
  let count = 0
  for (const groupId of itemGroupMap.value.values()) {
    if (groupId && trashDescendants.has(groupId)) count++
  }
  return count
})

// Count of groups inside the trash (excluding the trash group itself).
const trashGroupCount = computed(() => {
  const trashDescendants = groupsStore.descendantIdSet(TRASH_GROUP_ID)
  return trashDescendants.size - 1
})

const trashMenuOpen = ref(false)
const emptyTrashOpen = ref(false)

const trashContextMenuItems = computed<ContextMenuItem[][]>(() => [
  [
    {
      label: t('emptyTrash'),
      icon: 'i-lucide-trash-2',
      color: 'error' as const,
      disabled: trashItemCount.value === 0 && trashGroupCount.value === 0,
      onSelect: () => {
        emptyTrashOpen.value = true
      },
    },
  ],
])

const editorOpen = ref(false)
const editorMode = ref<'create' | 'edit'>('create')
const editingGroup = ref<SelectHaexPasswordsGroups | null>(null)
const createParentId = ref<string | null>(null)

const deleteOpen = ref(false)
const deletingGroup = ref<SelectHaexPasswordsGroups | null>(null)
const deletingGroupIsFinal = ref(false)

const onCreateGroup = () => {
  editingGroup.value = null
  editorMode.value = 'create'
  createParentId.value = null
  editorOpen.value = true
}

const onCreateChildGroup = (parentId: string) => {
  editingGroup.value = null
  editorMode.value = 'create'
  createParentId.value = parentId
  editorOpen.value = true
}

const onEditGroup = (group: SelectHaexPasswordsGroups) => {
  editingGroup.value = group
  editorMode.value = 'edit'
  createParentId.value = null
  editorOpen.value = true
}

const onDeleteGroup = (group: SelectHaexPasswordsGroups) => {
  deletingGroup.value = group
  deletingGroupIsFinal.value = groupsStore.isGroupInTrash(group.id)
  deleteOpen.value = true
}

const isRootDropTarget = ref(false)

const onRootDragOver = (event: DragEvent) => {
  if (!event.dataTransfer) return
  const types = event.dataTransfer.types
  if (!types.includes('application/x-haex-item') && !types.includes('application/x-haex-group'))
    return
  event.dataTransfer.dropEffect = 'move'
  isRootDropTarget.value = true
}

const onRootDragLeave = () => {
  isRootDropTarget.value = false
}

const onRootDrop = async (event: DragEvent) => {
  isRootDropTarget.value = false
  if (!event.dataTransfer) return

  const itemId = event.dataTransfer.getData('application/x-haex-item')
  if (itemId) {
    await setItemGroupAsync(itemId, null)
    return
  }

  const groupId = event.dataTransfer.getData('application/x-haex-group')
  if (groupId) {
    await groupsStore.moveGroupAsync(groupId, null)
  }
}
</script>

<i18n lang="yaml">
de:
  allPasswords: Alle Passwörter
  newGroup: Neuer Ordner
  dropHere: Hierhin
  trash: Papierkorb
  emptyTrash: Papierkorb leeren
en:
  allPasswords: All Passwords
  newGroup: New folder
  dropHere: Drop here
  trash: Trash
  emptyTrash: Empty trash
</i18n>
