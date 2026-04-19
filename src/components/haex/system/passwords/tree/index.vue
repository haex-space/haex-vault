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
        v-for="group in rootGroups"
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

    <HaexSystemPasswordsDialogGroupEditor
      v-model:open="editorOpen"
      :mode="editorMode"
      :group="editingGroup"
      :create-parent-id="createParentId"
    />

    <HaexSystemPasswordsDialogDeleteGroup
      v-model:open="deleteOpen"
      :group="deletingGroup"
    />
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexPasswordsGroups } from '~/database/schemas'

const { t } = useI18n()

const groupsStore = usePasswordsGroupsStore()
const { rootGroups, selectedGroupId } = storeToRefs(groupsStore)
const { selectGroup, setItemGroupAsync } = groupsStore

const editorOpen = ref(false)
const editorMode = ref<'create' | 'edit'>('create')
const editingGroup = ref<SelectHaexPasswordsGroups | null>(null)
const createParentId = ref<string | null>(null)

const deleteOpen = ref(false)
const deletingGroup = ref<SelectHaexPasswordsGroups | null>(null)

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
en:
  allPasswords: All Passwords
  newGroup: New folder
  dropHere: Drop here
</i18n>
