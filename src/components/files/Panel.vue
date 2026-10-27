<script setup lang="ts">
/**
 * One panel of the file browser (spec 044 US1, spec 062): the open folder of one place with its
 * search, its entries, its viewer and its dialogs. What the frame owns (the tab location, the
 * sources, the sidebar) comes through the panel context (`composables/useFilesSplit.ts`); the panel
 * never writes the location itself. Managing files (US3) lives in `composables/useFilesManage.ts`.
 */
import type { Entry } from '@bindings/Entry'
import type { FolderKeys } from '~/composables/useFilesKeyboard'
import { useFilesPanel } from '~/composables/useFilesSplit'
import {
  admits,
  type FilesFilter,
  formatFilter,
  isFiltering,
  parseFilter,
} from '~/lib/files/filters'
import { shownNameKey } from '~/lib/files/encrypted'
import { isPeerStorage, isRemoteStorage } from '~/lib/files/registry'
import type { SplitSide } from '~/lib/files/split'
import { childPath, parentPath, visibleEntries } from '~/lib/files/state'

const props = defineProps<{ side: SplitSide }>()

const { t } = useI18n()
const context = useFilesPanel(props.side)
const prefs = useFilesPrefs()
const { peerRootsAsync } = useFiles()

const place = context.place
const source = computed(() => place.value.source)
const path = computed(() => place.value.path)

// The start (no path yet) opens the home folder, or the first drive where there is none.
watchEffect(() => {
  if (path.value !== null) return
  if (source.value.kind === 'storage') {
    // Another own device starts at its home folder, as this one does (spec 056).
    if (isPeerStorage(source.value.storageId)) {
      void openPeerStartAsync(source.value.storageId)
      return
    }
    // A storage starts at its root.
    context.initialize({ source: source.value, path: '/' })
    return
  }
  const data = context.sources.value
  if (!data) return
  const home = data.known.find((known) => known.name === 'home')?.path
  const start = home ?? data.drives[0]?.path
  if (start) context.initialize({ source: source.value, path: start })
})

async function openPeerStartAsync(storageId: string) {
  let start = '/'
  try {
    const roots = await peerRootsAsync(storageId)
    start =
      roots.known.find((known) => known.name === 'home')?.path ??
      roots.drives[0]?.path ??
      '/'
  } catch {
    // Not reachable now: the root shows why when it fails to list.
  }
  const now = source.value
  if (
    path.value === null &&
    now.kind === 'storage' &&
    now.storageId === storageId
  )
    context.initialize({ source: now, path: start })
}

const folder = useFilesFolder(source, path)
// Shares (spec 056 US2).
const { sharing, sharingEntry, receivedShare, readOnlySource, share } =
  useFilesTabShares({ state: context.sources }, source)
// Android: "Zugriff auf alle Dateien" (FR-039); a grant in the settings loads the device at once.
const allAccess = useFilesAllAccess(() => {
  context.reloadSources()
  void folder.reload()
})
// --- Search and filter (US4): both live in the tab's location ---

const searchQuery = computed(() => place.value.q ?? '')
const filter = computed(() =>
  parseFilter({ t: place.value.t, s: place.value.s, d: place.value.d }),
)
const search = useFilesSearch(
  source,
  path,
  searchQuery,
  filter,
  prefs.showHidden,
)

function onSearch(query: string) {
  context.setQuery({ q: query.trim() || null })
}

function onFilter(next: FilesFilter) {
  context.setQuery(formatFilter(next))
}

const shown = computed(() => {
  const visible = visibleEntries(
    folder.entries.value,
    prefs.sort.value,
    prefs.showHidden.value,
  )
  if (!isFiltering(filter.value)) return visible
  const now = Date.now()
  return visible.filter((entry) => admits(entry, filter.value, now))
})

/** Opens a folder, with the filter kept and the search ended. */
function go(target: string, open?: string) {
  const { t: types, s: size, d: date } = place.value
  context.go({
    source: source.value,
    path: target,
    open,
    t: types,
    s: size,
    d: date,
  })
}

/** A search hit: a folder opens, a file opens in its folder's viewer. */
function openHit(entry: Entry) {
  if (entry.kind === 'dir') {
    go(entry.path)
    return
  }
  const parent = parentPath(entry.path)
  if (parent) go(parent, entry.name)
}

function goUp() {
  const parent = path.value ? parentPath(path.value) : null
  if (parent) go(parent)
}

// --- Managing files (US3) ---

const {
  selection,
  selecting,
  clearSelection,
  readOnly,
  folderForeign,
  canPaste,
  menuEntries,
  dimmed,
  onPress,
  onBand,
  onContext,
  runMenu,
  runBar,
  runKey,
  onDrop,
  importDropped,
  naming,
  renaming,
  namingEncrypted,
  noticing,
  ackNotice,
  deleting,
  pendingDelete,
  pendingForeign,
  deleteNow,
  openNoticing,
  openSystemAsync,
  confirmOpenAsync,
} = useFilesManage({
  source,
  path,
  shown,
  tabId: context.tabId,
  reload: () => folder.reload(),
  activate,
  readOnlySource,
  share,
})

function activate(entry: Entry) {
  if (entry.kind === 'dir') {
    if (!entry.noAccess) go(entry.path)
    return
  }
  context.setQuery({ open: entry.name }, { push: true })
}

/** The file in the viewer: its entry once the folder is listed, until then only name and path (a
 * restored tab opens the file before its folder arrives). */
const openEntry = computed(
  (): (Pick<Entry, 'name' | 'path'> & Partial<Entry>) | null => {
    const name = place.value.open
    if (!name || !path.value) return null
    return (
      folder.entries.value.find((entry) => entry.name === name) ?? {
        name,
        path: childPath(path.value, name),
      }
    )
  },
)

function closeViewer() {
  context.setQuery({ open: null })
}

function showSibling(entry: Entry) {
  context.setQuery({ open: entry.name })
}

const root = useTemplateRef<HTMLElement>('root')
const folderView = useTemplateRef<FolderKeys>('folderView')

// --- Keyboard (FR-017b) ---

const { onKeydown } = useFilesKeyboard({
  root,
  folderView,
  shown,
  selection,
  grid: computed(() => prefs.view.value === 'grid'),
  nameOf: (entry) => {
    const key = shownNameKey(entry)
    return key ? t(key) : entry.name
  },
  runKey,
  clearSelection,
  up: goUp,
  reload: () => void folder.reload(),
  toggleHidden: () => prefs.setShowHidden(!prefs.showHidden.value),
})

/** The last entry the keyboard or the pointer reached, else the first. */
function focus() {
  const entry = selection.value.anchor ?? shown.value[0]?.path
  if (entry) void folderView.value?.focusEntry(entry)
}

const unregister = context.register({
  side: props.side,
  target: () => ({
    source: source.value,
    path: path.value,
    readOnly: readOnly.value,
  }),
  reload: () => folder.reload(),
  focus,
  keydown: onKeydown,
})
onBeforeUnmount(unregister)

// --- Storages (US5) ---

const storageId = computed(() =>
  source.value.kind === 'storage' ? source.value.storageId : null,
)
const storageName = computed(
  () =>
    context.sources.value?.storages.find(
      (storage) => storage.id === storageId.value,
    )?.name ??
    context.sources.value?.devices.find(
      (device) => device.id === storageId.value,
    )?.name ??
    receivedShare.value?.name ??
    undefined,
)

/** A storage's credentials or host key failed: its settings can fix them (spec 038, 050). Another
 * own device has no storage settings. */
const settingsLink = computed(
  () =>
    storageId.value !== null &&
    !isRemoteStorage(storageId.value) &&
    (folder.error.value?.code === 'credentials' ||
      folder.error.value?.code === 'noAccess' ||
      folder.error.value?.code === 'hostKey'),
)

// A storage is not watched: it loads again when its tab comes to the front.
watch(context.tabActive, (active) => {
  if (active && storageId.value !== null) void folder.reload(true)
})
</script>

<template>
  <div
    ref="root"
    class="flex min-w-0 flex-1 flex-col"
    :data-files-panel="side"
    :data-active="context.active.value"
    @keydown="onKeydown"
  >
    <Teleport v-if="context.pathHost.value" defer :to="context.pathHost.value">
      <FilesPathBar
        :path="path"
        :root-label="storageName"
        :can-go-up="!!path && parentPath(path) !== null"
        @go="go"
        @up="goUp"
        @reload="folder.reload()"
      />
    </Teleport>
    <FilesSearchBar
      :query="searchQuery"
      :filter="filter"
      @search="onSearch"
      @filter="onFilter"
    />
    <main
      class="relative flex min-w-0 flex-1 flex-col"
      data-testid="files-main"
    >
      <FilesForeignHint
        v-if="folderForeign > 0 && !searchQuery"
        :count="folderForeign"
      />
      <FilesAllFilesAccess
        v-if="source.kind === 'device' && allAccess.granted.value === false"
        class="flex-1"
        @request="allAccess.requestAsync()"
      />
      <FilesSearchResults
        v-else-if="searchQuery && path"
        class="flex-1"
        :source="source"
        :root="path"
        :hits="search.hits.value"
        :running="search.running.value"
        :truncated="search.truncated.value"
        :dirs="search.dirs.value"
        :error="search.error.value"
        @open="openHit"
        @reveal="go"
      />
      <FilesDropTarget
        v-else
        class="flex-1"
        :disabled="readOnly || !path"
        @drop="importDropped"
      >
        <FilesMenu :entries="menuEntries" @run="runMenu">
          <FilesFolderView
            ref="folderView"
            :source="source"
            :entries="shown"
            :loading="folder.loading.value"
            :error="folder.error.value"
            :view="prefs.view.value"
            :folder="path ?? ''"
            :selected="selection.paths"
            :selection-anchor="selection.anchor"
            :dimmed="dimmed"
            :settings-link="settingsLink"
            :takes-focus="context.active.value"
            @press="onPress"
            @band="onBand"
            @open="activate"
            @clear="clearSelection()"
            @context="onContext"
            @drop="onDrop"
            @settings="context.openStorageSettings"
          />
        </FilesMenu>
      </FilesDropTarget>
      <!-- Below the list: appearing above it would move the rows between the two clicks of a
           double click. -->
      <FilesActionBar
        v-if="selecting || canPaste"
        :selected="selection.paths.length"
        :can-paste="canPaste"
        :read-only="readOnly"
        @run="runBar"
        @clear="clearSelection()"
      />
    </main>
    <!-- The viewer covers the panel area of the frame, not only this panel. -->
    <Teleport
      v-if="openEntry && context.viewerHost.value"
      defer
      :to="context.viewerHost.value"
    >
      <FilesViewer
        :source="source"
        :entry="openEntry"
        :siblings="shown"
        @close="closeViewer"
        @show="showSibling"
        @open-system="openSystemAsync"
      />
    </Teleport>
    <FilesNameDialog
      v-model:open="naming"
      :source="source"
      :folder="path ?? ''"
      :entry="renaming"
      :encrypted="namingEncrypted"
      @done="folder.reload()"
    />
    <FilesShareDialog
      v-model:open="sharing"
      :entry="sharingEntry"
      :source="source"
      :contacts="context.sources.value?.contacts ?? []"
    />
    <FilesEncryptedNotice v-model:open="noticing" @confirm="ackNotice" />
    <FilesOpenSystemNotice
      v-model:open="openNoticing"
      @confirm="confirmOpenAsync"
    />
    <FilesDeleteDialog
      v-model:open="deleting"
      :count="pendingDelete.length"
      :foreign="pendingForeign"
      @confirm="deleteNow"
    />
  </div>
</template>
