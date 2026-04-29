<template>
  <div class="flex flex-col gap-1 h-full">
    <button
      type="button"
      class="flex items-center gap-2 ps-3 pe-2 py-2 rounded-md text-[15px] transition-colors hover:bg-elevated min-h-12"
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

    <div class="flex-1 overflow-y-auto space-y-0.5">
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

    <UiButton
      :label="t('newGroup')"
      icon="i-lucide-folder-plus"
      color="neutral"
      variant="ghost"
      class="justify-start"
      @click="onCreateGroup"
    />

    <div class="border-t border-default pt-1">
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
    </div>

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
import type { SelectHaexPasswordsGroups } from '~/database/schemas'
import { TRASH_GROUP_ID } from '~/stores/passwords/groups'

const { t } = useI18n()

const groupsStore = usePasswordsGroupsStore()
const { rootGroups, selectedGroupId, trashGroup, itemGroupMap, itemCountByGroupId } = storeToRefs(groupsStore)
const { selectGroup, setItemGroupAsync } = groupsStore

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
en:
  allPasswords: All Passwords
  newGroup: New folder
  dropHere: Drop here
  trash: Trash
</i18n>
