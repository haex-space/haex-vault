<script setup lang="ts">
/**
 * One folder in the sidebar tree and, recursively, its subfolders (spec 034, US2, FR-009): expand
 * and collapse, the entry count, a menu with the actions that also work without a mouse (new
 * subfolder, edit, up, down) and drag and drop: a folder dropped on a sibling goes before it, entries
 * dropped on a folder move into it. The row only reports what happened; the sidebar acts. Spec 036
 * (FR-018): the menu is the folder menu of the list (also on right click) with Ausschneiden and
 * Einfügen, and a cut folder is dimmed.
 */
import type { GroupRow } from '@bindings/GroupRow'
import { FOLDER_MIME, ITEMS_MIME, parseItemsPayload } from '~/lib/passwords/dnd'
import { displayTitle } from '~/lib/passwords/format'
import { buildMenu, type MenuCommand } from '~/lib/passwords/menus'
import type { TreeNode } from '~/lib/passwords/tree'

const props = withDefaults(
  defineProps<{
    node: TreeNode<GroupRow>
    depth: number
    /** The ids of the folders on this level in display order. */
    siblingIds: string[]
    activeId: string | null
    /** The menu offers cut, copy and paste (Wallet's folders do not). */
    transfer?: boolean
  }>(),
  // Without the default an omitted Boolean prop reads as false and the password manager would lose
  // the three commands.
  { transfer: true },
)

const expanded = defineModel<Set<string>>('expanded', { required: true })

const emit = defineEmits<{
  dropFolder: [dragged: string, target: string]
  dropItems: [groupId: string, itemIds: string[]]
  /** A command of the folder menu (`lib/passwords/menus.ts`). */
  menu: [command: MenuCommand, group: GroupRow]
}>()

const { t } = useI18n()
const clipboard = usePasswordsClipboardStore()

const group = computed(() => props.node.group)
const name = computed(
  () => displayTitle(group.value.name) ?? t('passwords.folders.unknown'),
)
const isOpen = computed(() => expanded.value.has(group.value.id))
const hasChildren = computed(() => props.node.children.length > 0)
const position = computed(() => props.siblingIds.indexOf(group.value.id))
const dropping = ref(false)

const menuEntries = computed(() =>
  buildMenu({
    kind: 'treeFolder',
    transfer: props.transfer,
    ablageFilled: clipboard.filled,
    copyAvailable: true,
    canMoveUp: position.value > 0,
    canMoveDown:
      position.value !== -1 && position.value < props.siblingIds.length - 1,
  }),
)

function toggleOpen() {
  const next = new Set(expanded.value)
  if (next.has(group.value.id)) next.delete(group.value.id)
  else next.add(group.value.id)
  expanded.value = next
}

function onDragStart(event: DragEvent) {
  event.dataTransfer?.setData(FOLDER_MIME, group.value.id)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

function accepts(event: DragEvent): boolean {
  const types = event.dataTransfer?.types ?? []
  return types.includes(ITEMS_MIME) || types.includes(FOLDER_MIME)
}

function onDragOver(event: DragEvent) {
  if (!accepts(event)) return
  event.preventDefault()
  dropping.value = true
}

function onDrop(event: DragEvent) {
  dropping.value = false
  const folder = event.dataTransfer?.getData(FOLDER_MIME)
  const items = event.dataTransfer?.getData(ITEMS_MIME)
  if (folder) {
    event.preventDefault()
    emit('dropFolder', folder, group.value.id)
  } else if (items) {
    const ids = parseItemsPayload(items)
    if (ids.length === 0) return
    event.preventDefault()
    emit('dropItems', group.value.id, ids)
  }
}
</script>

<template>
  <li :data-testid="`passwords-folder-${group.id}`">
    <PasswordsEntryMenu
      :entries="menuEntries"
      @run="emit('menu', $event, group)"
    >
      <div
        class="group flex min-h-9 items-center gap-1 rounded-lg pr-1 hover:bg-foreground/5"
        :class="[
          activeId === group.id ? 'bg-foreground/10 font-medium' : '',
          dropping ? 'ring-2 ring-primary' : '',
          clipboard.dimmed.has(group.id) ? 'opacity-50 grayscale' : '',
        ]"
        :style="{ paddingLeft: `${depth * 12 + 4}px` }"
        draggable="true"
        @dragstart="onDragStart"
        @dragover="onDragOver"
        @dragleave="dropping = false"
        @drop="onDrop"
      >
        <button
          type="button"
          class="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground"
          :class="hasChildren ? '' : 'invisible'"
          :aria-label="
            isOpen
              ? t('passwords.folders.collapse')
              : t('passwords.folders.expand')
          "
          :aria-expanded="isOpen"
          @click="toggleOpen"
        >
          <Icon
            :name="isOpen ? 'lucide:chevron-down' : 'lucide:chevron-right'"
            class="size-4"
          />
        </button>
        <button
          type="button"
          class="flex min-w-0 flex-1 items-center gap-2 py-1.5 text-left"
          @click="emit('menu', 'open', group)"
        >
          <span
            class="size-4 shrink-0"
            :style="group.color ? { color: group.color } : undefined"
          >
            <PasswordsEntryIcon :value="group.icon" fallback="lucide:folder" />
          </span>
          <span class="min-w-0 flex-1 truncate">{{ name }}</span>
          <span v-if="node.itemCount" class="text-xs text-muted-foreground">{{
            node.itemCount
          }}</span>
        </button>
        <PasswordsEntryMenuButton
          :entries="menuEntries"
          :label="t('passwords.folders.menu', { name })"
          tabindex="0"
          :data-testid="`passwords-folder-menu-${group.id}`"
          @run="emit('menu', $event, group)"
        />
      </div>
    </PasswordsEntryMenu>
    <ul v-if="hasChildren && isOpen" class="flex flex-col">
      <PasswordsTreeItem
        v-for="child in node.children"
        :key="child.group.id"
        v-model:expanded="expanded"
        :node="child"
        :depth="depth + 1"
        :sibling-ids="node.children.map((c) => c.group.id)"
        :active-id="activeId"
        :transfer="transfer"
        @drop-folder="(dragged, target) => emit('dropFolder', dragged, target)"
        @drop-items="(id, ids) => emit('dropItems', id, ids)"
        @menu="(command, child) => emit('menu', command, child)"
      />
    </ul>
  </li>
</template>
