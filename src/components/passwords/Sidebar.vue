<script setup lang="ts">
/**
 * The sidebar of the password manager (spec 034, US2, FR-009, FR-011): all entries, the folder
 * tree with expand and collapse, the trash and the actions for folders (the tag filter sits
 * beside the search, `TagFilter.vue`; generator, import and tags are in the menu of the toolbar).
 * Folders reorder by drag and drop and by the menu actions "up" and "down" (also with keyboard
 * and touch); entries dragged from the list move into a folder. Only ids travel.
 */
import { toast } from 'vue-sonner'
import type { GroupRow } from '@bindings/GroupRow'
import type { CopyRequest } from '~/composables/usePasswordsActions'
import { FOLDER_MIME, ITEMS_MIME, parseItemsPayload } from '~/lib/passwords/dnd'
import type { Target } from '@bindings/Target'
import { emptyTargets as emptyTargetsOf } from '~/lib/passwords/scope'
import { buildMenu, type MenuCommand } from '~/lib/passwords/menus'
import {
  buildTree,
  findNode,
  moveBefore,
  moveDown,
  moveUp,
  TRASH_GROUP_ID,
} from '~/lib/passwords/tree'

const emit = defineEmits<{
  /** The user picked a place: the frame closes the overlay of a narrow window. */
  navigated: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = usePasswordsStore()
const { reorderGroupsAsync } = usePasswords()
const actions = usePasswordsActions()

const tree = computed(() => buildTree(store.groups, store.loginHeaders))
const expanded = ref(new Set<string>())
const folderDialog = ref(false)
const editing = ref<GroupRow | null>(null)
const parentForNew = ref<string | null>(null)
const rootDropping = ref(false)
const deleteOpen = ref(false)
const deleteTargets = ref<Target[]>([])

const place = computed(() => {
  const path = router.route.path
  if (path === '/trash') return { kind: 'trash' as const }
  if (path.startsWith('/folder/'))
    return { kind: 'folder' as const, id: router.route.params.id ?? '' }
  if (path === '/') return { kind: 'all' as const }
  return { kind: 'other' as const }
})
const activeFolderId = computed(() =>
  place.value.kind === 'folder' ? place.value.id : null,
)
const activeTag = computed(() => router.route.query.tag ?? null)

function go(path: string) {
  router.push(path)
  emit('navigated')
}

function newFolder(parentId: string | null) {
  editing.value = null
  parentForNew.value = parentId
  folderDialog.value = true
  if (parentId) expanded.value = new Set(expanded.value).add(parentId)
}

function askDeleteFolder(group: GroupRow) {
  deleteTargets.value = [{ kind: 'group', id: group.id }]
  deleteOpen.value = true
}

/** After a folder went to the trash the window leaves it if it was showing it. */
function afterFolderDelete() {
  if (
    activeFolderId.value &&
    deleteTargets.value.some((t) => t.id === activeFolderId.value)
  ) {
    router.replace('/')
  }
}

function editFolder(group: GroupRow) {
  editing.value = group
  folderDialog.value = true
}

function siblingsOf(parentId: string | null): string[] {
  const level =
    parentId === null
      ? tree.value.roots
      : (findNode(tree.value.roots, parentId)?.children ?? [])
  return level.map((node) => node.group.id)
}

function parentOf(id: string): string | null {
  return store.groups.find((group) => group.id === id)?.parentId ?? null
}

async function reorderAsync(parentId: string | null, ordered: string[]) {
  try {
    await reorderGroupsAsync(parentId, ordered)
    await store.quietReloadAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function moveFolderAsync(id: string, direction: 'up' | 'down') {
  const parentId = parentOf(id)
  const ids = siblingsOf(parentId)
  const next = direction === 'up' ? moveUp(ids, id) : moveDown(ids, id)
  await reorderAsync(parentId, next)
}

async function dropFolderAsync(dragged: string, target: string) {
  const parentId = parentOf(target)
  // Dropping on a folder of another level does nothing: nesting is done by moving entries.
  if (parentOf(dragged) !== parentId) return
  await reorderAsync(
    parentId,
    moveBefore(siblingsOf(parentId), dragged, target),
  )
}

/** Entries and folders dropped on a folder or on "Alle Einträge" move there (spec 036, FR-020). */
async function dropItemsAsync(groupId: string | null, ids: string[]) {
  await actions.moveIdsAsync(ids, groupId)
}

/** A command of a folder's menu in the tree (spec 036, FR-018). */
async function runFolderCommand(command: MenuCommand, group: GroupRow) {
  switch (command) {
    case 'open':
      go(`/folder/${group.id}`)
      break
    case 'edit':
      editFolder(group)
      break
    case 'newSubfolder':
      newFolder(group.id)
      break
    case 'moveUp':
    case 'moveDown':
      await moveFolderAsync(group.id, command === 'moveUp' ? 'up' : 'down')
      break
    case 'cut':
      actions.cut([group.id])
      break
    case 'copy':
      actions.copy([group.id])
      break
    case 'paste':
      await actions.pasteAsync(group.id, (request) => {
        copyRequest.value = request
        copyOpen.value = true
      })
      break
    case 'delete':
      askDeleteFolder(group)
      break
    default:
      break
  }
}

const emptyTrashOpen = ref(false)
const copyRequest = ref<CopyRequest | null>(null)
const copyOpen = ref(false)
const trashedFolders = computed(
  () =>
    tree.value.trash.groups.filter((group) => group.id !== TRASH_GROUP_ID)
      .length,
)
const trashedItemIds = computed(() => {
  const ids = new Set(tree.value.trash.groups.map((group) => group.id))
  return store.loginHeaders
    .filter((header) => header.groupId !== null && ids.has(header.groupId))
    .map((header) => header.id)
})
/** What "Empty trash" removes: the folders and the logins deleted directly. The cards and tickets
 * in the trash are Wallet's to empty. */
const emptyTargets = computed(() =>
  emptyTargetsOf('login', TRASH_GROUP_ID, store.groups, store.headers),
)
const trashMenu = computed(() =>
  buildMenu({
    kind: 'trashNode',
    trashEmpty: tree.value.trash.itemCount === 0 && trashedFolders.value === 0,
  }),
)

function onRootDragOver(event: DragEvent) {
  const types = event.dataTransfer?.types ?? []
  if (!types.includes(ITEMS_MIME) && !types.includes(FOLDER_MIME)) return
  event.preventDefault()
  rootDropping.value = true
}

async function onRootDrop(event: DragEvent) {
  rootDropping.value = false
  const folder = event.dataTransfer?.getData(FOLDER_MIME)
  const ids = folder
    ? [folder]
    : parseItemsPayload(event.dataTransfer?.getData(ITEMS_MIME))
  if (ids.length === 0) return
  event.preventDefault()
  await dropItemsAsync(null, ids)
}
</script>

<template>
  <div
    class="flex h-full min-h-0 flex-col gap-3 overflow-y-auto p-2"
    data-testid="passwords-sidebar-content"
  >
    <ul class="flex flex-col gap-0.5">
      <li>
        <SidebarRow
          icon="lucide:key-round"
          :active="place.kind === 'all' && !activeTag"
          :class="rootDropping ? 'ring-2 ring-primary' : ''"
          data-testid="passwords-all"
          @click="go('/')"
          @dragover="onRootDragOver"
          @dragleave="rootDropping = false"
          @drop="onRootDrop"
        >
          {{ t('passwords.sidebar.all') }}
          <template #trailing>
            <span class="text-xs text-muted-foreground">{{
              store.loginHeaders.length - tree.trash.itemCount
            }}</span>
          </template>
        </SidebarRow>
      </li>
      <PasswordsTreeItem
        v-for="node in tree.roots"
        :key="node.group.id"
        v-model:expanded="expanded"
        :node="node"
        :depth="0"
        :sibling-ids="tree.roots.map((root) => root.group.id)"
        :active-id="activeFolderId"
        @drop-folder="dropFolderAsync"
        @drop-items="dropItemsAsync"
        @menu="runFolderCommand"
      />
    </ul>

    <div class="mt-auto flex flex-col gap-1">
      <PasswordsEntryMenu
        :entries="trashMenu"
        @run="(command) => command === 'emptyTrash' && (emptyTrashOpen = true)"
      >
        <SidebarRow
          icon="lucide:trash-2"
          icon-class="text-destructive"
          :active="place.kind === 'trash'"
          data-testid="passwords-trash"
          @click="go('/trash')"
        >
          {{ t('passwords.trash.title') }}
          <template #trailing>
            <span
              v-if="tree.trash.itemCount"
              class="text-xs text-muted-foreground"
              >{{ tree.trash.itemCount }}</span
            >
          </template>
        </SidebarRow>
      </PasswordsEntryMenu>
    </div>

    <PasswordsFolderDialog
      v-model:open="folderDialog"
      :group="editing"
      :parent-id="parentForNew"
    />
    <PasswordsDeleteDialog
      v-model:open="deleteOpen"
      :targets="deleteTargets"
      @done="afterFolderDelete"
    />
    <PasswordsCopyDialog v-model:open="copyOpen" :request="copyRequest" />
    <PasswordsEmptyTrashDialog
      v-model:open="emptyTrashOpen"
      :entries="tree.trash.itemCount"
      :folders="trashedFolders"
      :item-ids="trashedItemIds"
      :targets="emptyTargets"
    />
  </div>
</template>
