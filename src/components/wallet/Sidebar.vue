<script setup lang="ts">
/**
 * The sidebar of Wallet (spec 092, Phase W): all passes, Wallet's own folders as a tree with expand
 * and collapse, "New folder" and the trash. The folders are the subtree below the fixed root
 * `wallet` (`lib/wallet/folders.ts`); a folder reorders with the menu actions "up" and "down" or by
 * drag and drop, and passes dragged from the list move into a folder (or to "All", out of every
 * folder). Only ids travel. The folder menu has no cut, copy or paste.
 */
import { toast } from 'vue-sonner'
import type { GroupRow } from '@bindings/GroupRow'
import type { Target } from '@bindings/Target'
import { FOLDER_MIME, ITEMS_MIME, parseItemsPayload } from '~/lib/passwords/dnd'
import type { MenuCommand } from '~/lib/passwords/menus'
import { inScope } from '~/lib/passwords/scope'
import {
  buildTree,
  findNode,
  isInTrash,
  moveBefore,
  moveDown,
  moveUp,
  trashGroupIds,
} from '~/lib/passwords/tree'
import { realParentOf, WALLET_GROUP_ID } from '~/lib/wallet/folders'

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

/** The passes outside the trash: the ones Wallet counts and shows. */
const passes = computed(() => {
  const trash = trashGroupIds(store.groups)
  return inScope(store.headers, 'pass').filter(
    (header) => !isInTrash(header.groupId, trash),
  )
})
const trashedPasses = computed(
  () => inScope(store.headers, 'pass').length - passes.value.length,
)
const tree = computed(() => buildTree(store.walletFolders, passes.value))
const expanded = ref(new Set<string>())
const folderDialog = ref(false)
const editing = ref<GroupRow | null>(null)
const parentForNew = ref<string>(WALLET_GROUP_ID)
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

function go(path: string) {
  void router.push(path)
  emit('navigated')
}

function newFolder(parentId: string | null) {
  editing.value = null
  parentForNew.value = realParentOf(parentId)
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
    deleteTargets.value.some((target) => target.id === activeFolderId.value)
  ) {
    void router.replace('/')
  }
}

/** The folders on one level of the tree in display order (`null` is the top of Wallet). */
function siblingsOf(parentId: string | null): string[] {
  const level =
    parentId === null
      ? tree.value.roots
      : (findNode(tree.value.roots, parentId)?.children ?? [])
  return level.map((node) => node.group.id)
}

/** The parent as the tree shows it: `null` for a folder at the top of Wallet. */
function parentOf(id: string): string | null {
  const parent = store.groups.find((group) => group.id === id)?.parentId ?? null
  return parent === WALLET_GROUP_ID ? null : parent
}

async function reorderAsync(parentId: string | null, ordered: string[]) {
  try {
    await reorderGroupsAsync(realParentOf(parentId), ordered)
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
  // Dropping on a folder of another level does nothing: nesting is done by moving.
  if (parentOf(dragged) !== parentId) return
  await reorderAsync(
    parentId,
    moveBefore(siblingsOf(parentId), dragged, target),
  )
}

/** Passes dropped on a folder move into it. */
async function dropItemsAsync(groupId: string, ids: string[]) {
  await actions.moveIdsAsync(ids, groupId)
}

/** A command of a folder's menu in the tree. */
async function runFolderCommand(command: MenuCommand, group: GroupRow) {
  switch (command) {
    case 'open':
      go(`/folder/${group.id}`)
      break
    case 'edit':
      editing.value = group
      folderDialog.value = true
      break
    case 'newSubfolder':
      newFolder(group.id)
      break
    case 'moveUp':
    case 'moveDown':
      await moveFolderAsync(group.id, command === 'moveUp' ? 'up' : 'down')
      break
    case 'delete':
      askDeleteFolder(group)
      break
    default:
      break
  }
}

function onRootDragOver(event: DragEvent) {
  const types = event.dataTransfer?.types ?? []
  if (!types.includes(ITEMS_MIME) && !types.includes(FOLDER_MIME)) return
  event.preventDefault()
  rootDropping.value = true
}

/** Dropped on "All": passes leave their folder (the top level), a folder goes to the top of Wallet. */
async function onRootDrop(event: DragEvent) {
  rootDropping.value = false
  const folder = event.dataTransfer?.getData(FOLDER_MIME)
  if (folder) {
    event.preventDefault()
    await actions.moveIdsAsync([folder], WALLET_GROUP_ID)
    return
  }
  const ids = parseItemsPayload(event.dataTransfer?.getData(ITEMS_MIME))
  if (ids.length === 0) return
  event.preventDefault()
  await actions.moveIdsAsync(ids, null)
}
</script>

<template>
  <div
    class="flex h-full min-h-0 flex-col gap-3 overflow-y-auto p-2"
    data-testid="wallet-sidebar-content"
  >
    <ul class="flex flex-col gap-0.5">
      <li>
        <SidebarRow
          icon="lucide:wallet"
          :active="place.kind === 'all'"
          :class="rootDropping ? 'ring-2 ring-primary' : ''"
          data-testid="wallet-all"
          @click="go('/')"
          @dragover="onRootDragOver"
          @dragleave="rootDropping = false"
          @drop="onRootDrop"
        >
          {{ t('wallet.sidebar.all') }}
          <template #trailing>
            <span class="text-xs text-muted-foreground">{{
              passes.length
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
        :transfer="false"
        @drop-folder="dropFolderAsync"
        @drop-items="dropItemsAsync"
        @menu="runFolderCommand"
      />
      <li>
        <SidebarRow
          icon="lucide:folder-plus"
          data-testid="wallet-new-folder"
          @click="newFolder(null)"
        >
          {{ t('wallet.sidebar.newFolder') }}
        </SidebarRow>
      </li>
    </ul>

    <div class="mt-auto flex flex-col gap-1">
      <SidebarRow
        icon="lucide:trash-2"
        icon-class="text-destructive"
        :active="place.kind === 'trash'"
        data-testid="wallet-trash"
        @click="go('/trash')"
      >
        {{ t('wallet.trash') }}
        <template #trailing>
          <span v-if="trashedPasses" class="text-xs text-muted-foreground">{{
            trashedPasses
          }}</span>
        </template>
      </SidebarRow>
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
  </div>
</template>
