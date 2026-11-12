<script setup lang="ts">
/**
 * The list of entries (spec 034, US1, FR-007): every entry outside the trash, filtered by the
 * search text of the place (`?q=`), in the boxed list style of the other holzi views. While the
 * overview has not been loaded once it shows a spinner instead of the empty state.
 *
 * Spec 036 (US3, US4): above it the breadcrumbs, or the selection bar while rows are selected (the
 * breadcrumbs again while entries are dragged, so a selection can be dropped on them), and the bar
 * of the Ablage; the subfolders of the open folder as rows that are selected together with
 * entries; the context menus of the rows and of the empty area; one tab stop that the arrow keys
 * move; and the list shortcuts, which the frame hands over while the list shows.
 */
import { toast } from 'vue-sonner'
import type { GroupRow } from '@bindings/GroupRow'
import type { CopyRequest } from '~/composables/usePasswordsActions'
import type { BreadcrumbPlace } from '~/lib/passwords/breadcrumb'
import { ITEMS_MIME } from '~/lib/passwords/dnd'
import { buildMenu, type MenuCommand } from '~/lib/passwords/menus'
import { filterHeaders, sortByTitle } from '~/lib/passwords/search'
import {
  buildTree,
  findNode,
  isInTrash,
  trashGroupIds,
} from '~/lib/passwords/tree'

const { t } = useI18n()
const router = useTabRouter()
const store = usePasswordsStore()
// A failed (quiet) reload of the overview shows as a toast, once for all open lists.
watch(
  () => store.lastError,
  (message) => {
    if (message) toast.error(message, { id: 'passwords-load-failed' })
  },
)
const selection = usePasswordsSelectionStore()
const clipboard = usePasswordsClipboardStore()
const actions = usePasswordsActions()

/** Entries or folders are being dragged: the breadcrumbs take the selection bar's place as targets. */
const dragging = ref(false)
useEventListener(document, 'dragstart', (event: DragEvent) => {
  dragging.value = event.dataTransfer?.types.includes(ITEMS_MIME) ?? false
})
useEventListener(document, 'dragend', () => (dragging.value = false))
useEventListener(document, 'drop', () => (dragging.value = false))

/** The open folder (`/folder/:id`), or `null` for the top level. */
const folderId = computed(() => router.route.params.id ?? null)
const tagId = computed(() => router.route.query.tag ?? '')
const searching = computed(() => Boolean(router.route.query.q))
/** A paste goes into the open folder or the top level, not into a search or a tag view. */
const pasteHere = computed(() => !searching.value && !tagId.value)

const crumbPlace = computed<BreadcrumbPlace>(() => {
  if (searching.value) return { kind: 'search' }
  if (tagId.value) {
    const tag = store.displayTags.find((candidate) =>
      candidate.ids.includes(tagId.value),
    )
    return { kind: 'tag', name: tag?.name ?? '' }
  }
  return { kind: 'folder', id: folderId.value }
})

const tree = computed(() => buildTree(store.loginGroups, store.loginHeaders))

/** The subfolders of the open folder (the top-level folders at the top); none in a search or a tag
 * view, which list entries only. */
const folders = computed(() => {
  if (!pasteHere.value) return []
  if (folderId.value === null) return tree.value.roots
  return findNode(tree.value.roots, folderId.value)?.children ?? []
})

const visible = computed(() => {
  const trash = trashGroupIds(store.groups)
  // A search looks through every folder, not only the open one.
  const live = store.loginHeaders.filter(
    (header) =>
      !isInTrash(header.groupId, trash) &&
      (searching.value ||
        folderId.value === null ||
        header.groupId === folderId.value),
  )
  // Sorted first, so hits that rank alike keep the title order.
  return filterHeaders(sortByTitle(live), {
    query: router.route.query.q ?? '',
    ...(tagId.value ? { tagIds: store.tagIdsOf(tagId.value) } : {}),
  })
})

// ponytail: the entries mount in pages of `PAGE` as the list scrolls near its end, so opening the
// manager or a folder mounts at most one page. Ceiling: after scrolling to the end of a large vault
// every row is mounted again; the upgrade is a virtual list that unmounts rows out of view.
const PAGE = 100
const shownCount = ref(PAGE)
const shown = computed(() => visible.value.slice(0, shownCount.value))

/** The rows as the list shows them: folders first, then entries. */
const visibleIds = computed(() => [
  ...folders.value.map((node) => node.group.id),
  ...visible.value.map((header) => header.id),
])
const groupIds = computed(() => new Set(store.groups.map((group) => group.id)))
const isFolder = (id: string) => groupIds.value.has(id)

// What no longer shows (deleted elsewhere, filtered out) leaves the selection, and it ends when the
// folder changes or an entry opens (FR-011).
watch(visibleIds, (ids) => selection.keepOnly(new Set(ids)))
watch(
  () => router.route.path,
  () => {
    selection.clear()
    keepFocus()
  },
)
onMounted(() => keepFocus())
onBeforeUnmount(() => selection.clear())

function open(id: string) {
  router.push(isFolder(id) ? `/folder/${id}` : `/entry/${id}`)
}

function activate(id: string, event: MouseEvent) {
  focusedId.value = id
  if (event.shiftKey) selection.rangeTo(visibleIds.value, id)
  else if (event.ctrlKey || event.metaKey || selection.active)
    selection.toggleId(id)
  else open(id)
}

function longPress(id: string) {
  if (selection.active) selection.toggleId(id)
  else selection.selectEvery([id])
}

// --- Menus -------------------------------------------------------------------------------------

/** A menu opened inside the selection applies to all of it, outside it to the row alone. */
function idsFor(id: string): string[] {
  return selection.active && selection.isIdSelected(id)
    ? [...selection.ids]
    : [id]
}

function entryMenu(id: string) {
  const header = store.headersById.get(id)
  return buildMenu({
    kind: 'entry',
    ablageFilled: clipboard.filled,
    selectionSize: idsFor(id).length,
    copyAvailable: true,
    hasUsername: Boolean(header?.username),
    hasPassword: header?.hasPassword ?? false,
  })
}

function folderMenu(id: string) {
  return buildMenu({
    kind: 'folder',
    ablageFilled: clipboard.filled,
    selectionSize: idsFor(id).length,
    copyAvailable: true,
  })
}

const emptyMenu = computed(() =>
  buildMenu({
    kind: 'empty',
    ablageFilled: clipboard.filled,
    pasteHere: pasteHere.value,
  }),
)

/** A right click outside an existing selection makes the row the selection (FR-018). */
function onMenuOpen(id: string) {
  if (selection.active && !selection.isIdSelected(id))
    selection.selectEvery([id])
}

const deleteOpen = ref(false)
const deleteIds = ref<string[]>([])
const folderDialog = ref(false)
const copyRequest = ref<CopyRequest | null>(null)
const copyOpen = ref(false)

/** A paste of a copied Ablage opens the copy dialog of this window (spec 036, FR-015). */
function openCopy(request: CopyRequest) {
  copyRequest.value = request
  copyOpen.value = true
}
const editing = ref<GroupRow | null>(null)
const parentForNew = ref<string | null>(null)

function askDelete(ids: readonly string[]) {
  if (ids.length === 0) return
  deleteIds.value = [...ids]
  deleteOpen.value = true
}

function newFolder(parentId: string | null) {
  editing.value = null
  parentForNew.value = parentId
  folderDialog.value = true
}

/** `id` is the row of the menu, `null` for the empty area. */
async function run(command: MenuCommand, id: string | null) {
  const ids = id === null ? [] : idsFor(id)
  switch (command) {
    case 'open':
      if (id) open(id)
      break
    case 'edit':
      editing.value = store.groups.find((group) => group.id === id) ?? null
      folderDialog.value = editing.value !== null
      break
    case 'newSubfolder':
      newFolder(id)
      break
    case 'newFolder':
      newFolder(folderId.value)
      break
    case 'newEntry':
      router.push('/entry/new')
      break
    case 'copyUsername':
    case 'copyPassword':
      if (id)
        await actions.copyValueAsync(
          id,
          command === 'copyUsername' ? 'username' : 'password',
        )
      break
    case 'cut':
      actions.cut(ids)
      selection.clear()
      break
    case 'copy':
      actions.copy(ids)
      selection.clear()
      break
    case 'paste':
      // Into the folder of the menu, or for the empty area into the open one.
      await actions.pasteAsync(id ?? folderId.value, openCopy)
      break
    case 'delete':
      askDelete(ids)
      break
    default:
      break
  }
}

// --- Keyboard ----------------------------------------------------------------------------------

const area = useTemplateRef<HTMLElement>('area')
const listEl = useTemplateRef<HTMLElement>('listEl')
const { focusedId, tabStopId, keepFocus } = usePasswordsListKeys({
  area,
  list: listEl,
  visibleIds,
  isFolder,
  open,
  askDelete,
  pasteTarget: computed(() => (pasteHere.value ? folderId.value : undefined)),
  openCopy,
})

const rowCount = computed(() => visibleIds.value.length)

// Another place starts with one page, at the top (the list stays mounted between places). The
// focus starts anew as well: the entry focused before may lie far past the first page there.
watch(
  () => [router.route.path, router.route.query.q, router.route.query.tag],
  () => {
    shownCount.value = PAGE
    focusedId.value = null
    area.value?.scrollTo({ top: 0 })
  },
)

useInfiniteScroll(
  area,
  () => {
    shownCount.value += PAGE
  },
  {
    distance: 800,
    canLoadMore: () => shownCount.value < visible.value.length,
  },
)

// The keyboard can move the focus to a row past the shown page; that page then mounts.
watch(focusedId, (id) => {
  const index = id === null ? -1 : visible.value.findIndex((h) => h.id === id)
  if (index >= shownCount.value) shownCount.value = index + 1
})
</script>

<template>
  <PasswordsEntryMenu :entries="emptyMenu" @run="run($event, null)">
    <div
      ref="area"
      class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 outline-none @md:px-6"
      tabindex="-1"
      data-testid="passwords-list-area"
    >
      <div class="mx-auto flex w-full max-w-3xl flex-col gap-3">
        <div class="flex min-h-12 min-w-0 items-center pt-1">
          <Transition
            mode="out-in"
            enter-active-class="transition-opacity duration-150 motion-reduce:transition-none"
            leave-active-class="transition-opacity duration-100 motion-reduce:transition-none"
            enter-from-class="opacity-0"
            leave-to-class="opacity-0"
          >
            <div
              v-if="selection.active && !dragging"
              key="selection"
              class="flex min-w-0 flex-1"
            >
              <PasswordsSelectionBar
                :visible-ids="visibleIds"
                @delete="askDelete(selection.ids)"
              />
            </div>
            <div v-else key="breadcrumbs" class="flex min-w-0 flex-1">
              <PasswordsBreadcrumbs class="flex-1" :place="crumbPlace" />
            </div>
          </Transition>
        </div>
        <PasswordsClipboardBar
          v-if="clipboard.filled && !selection.active"
          :can-paste="pasteHere"
          @paste="actions.pasteAsync(folderId, openCopy)"
        />
        <PasswordsLoadState
          v-if="!store.hasLoadedOnce"
          testid="passwords-loading"
        />
        <p
          v-else-if="rowCount === 0"
          class="py-10 text-center text-muted-foreground"
          data-testid="passwords-empty"
        >
          {{ searching ? t('passwords.noMatches') : t('passwords.empty') }}
        </p>
        <div
          v-else
          ref="listEl"
          class="flex flex-col gap-3"
          data-passwords-list
        >
          <SettingsGroup
            v-if="folders.length"
            :label="t('passwords.list.folders')"
          >
            <PasswordsFolderRow
              v-for="node in folders"
              :key="node.group.id"
              :group="node.group"
              :item-count="node.itemCount"
              :selected="selection.isIdSelected(node.group.id)"
              :selecting="selection.active"
              :dimmed="clipboard.dimmed.has(node.group.id)"
              :tab-stop="tabStopId === node.group.id"
              :menu-for="folderMenu"
              @activate="activate(node.group.id, $event)"
              @long-press="longPress(node.group.id)"
              @focus="focusedId = node.group.id"
              @menu-open="onMenuOpen(node.group.id)"
              @menu="run($event, node.group.id)"
              @drop="actions.moveIdsAsync($event, node.group.id)"
            />
          </SettingsGroup>
          <SettingsGroup
            v-if="visible.length"
            :label="folders.length ? t('passwords.list.entries') : undefined"
          >
            <PasswordsListItem
              v-for="(header, index) in shown"
              :key="header.id"
              :header="header"
              :position="index + 1"
              :count="visible.length"
              :selected="selection.isIdSelected(header.id)"
              :selecting="selection.active"
              :dimmed="clipboard.dimmed.has(header.id)"
              :tab-stop="tabStopId === header.id"
              :menu-for="entryMenu"
              @activate="activate(header.id, $event)"
              @long-press="longPress(header.id)"
              @focus="focusedId = header.id"
              @menu-open="onMenuOpen(header.id)"
              @menu="run($event, header.id)"
            />
          </SettingsGroup>
        </div>
      </div>
      <PasswordsDeleteDialog
        v-model:open="deleteOpen"
        :targets="actions.targetsOf(deleteIds)"
        @done="selection.clear()"
      />
      <PasswordsCopyDialog v-model:open="copyOpen" :request="copyRequest" />
      <PasswordsFolderDialog
        v-model:open="folderDialog"
        :group="editing"
        :parent-id="parentForNew"
      />
    </div>
  </PasswordsEntryMenu>
</template>
