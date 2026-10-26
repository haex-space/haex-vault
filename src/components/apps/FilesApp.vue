<script lang="ts">
import type { Entry } from '@bindings/Entry'
import type { FolderKeys } from '~/composables/useFilesKeyboard'
import {
  admits,
  type FilesFilter,
  formatFilter,
  isFiltering,
  parseFilter,
} from '~/lib/files/filters'
import { shownNameKey } from '~/lib/files/encrypted'
import {
  filesLocation,
  isPeerStorage,
  isRemoteStorage,
  parseFilesPlace,
} from '~/lib/files/registry'
import { childPath, parentPath, visibleEntries } from '~/lib/files/state'

/** How many file browser frames are open, across all windows of the process. */
let openFrames = 0
</script>

<script setup lang="ts">
/**
 * The file browser frame (spec 044, US1), laid out like the password manager: a toolbar, below it
 * the sidebar (known places, drives) and the open folder as list or grid, with the viewer over it.
 * Where the tab stands is its location (`lib/files/registry.ts`), so moving between folders keeps
 * this component mounted and session restore (spec 022) brings tab and open file back.
 * Managing files (US3) lives in `composables/useFilesManage.ts`.
 */

const { t } = useI18n()
const router = useTabRouter()
const prefs = useFilesPrefs()
const { sourcesAsync, releaseTabAsync, peerRootsAsync } = useFiles()

const place = computed(() => parseFilesPlace(router.route))
const source = computed(
  () => place.value?.source ?? { kind: 'device' as const },
)
const path = computed(() => place.value?.path ?? null)

const sources = useAsyncState(() => sourcesAsync(), null)

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
    router.replace(filesLocation({ source: source.value, path: '/' }))
    return
  }
  const data = sources.state.value
  if (!data) return
  const home = data.known.find((known) => known.name === 'home')?.path
  const start = home ?? data.drives[0]?.path
  if (start)
    router.replace(filesLocation({ source: source.value, path: start }))
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
    router.replace(filesLocation({ source: now, path: start }))
}

const folder = useFilesFolder(source, path)
// Shares (spec 056 US2).
const {
  sharing,
  sharingEntry,
  receivedShare,
  readOnlySource,
  share,
  queryContactAsync,
} = useFilesTabShares(sources, source)
// Android: "Zugriff auf alle Dateien" (FR-039); a grant in the settings loads the device at once.
const allAccess = useFilesAllAccess(() => {
  void sources.execute()
  void folder.reload()
})
// --- Search and filter (US4): both live in the tab's location ---

const searchQuery = computed(() => place.value?.q ?? '')
const filter = computed(() =>
  parseFilter({ t: place.value?.t, s: place.value?.s, d: place.value?.d }),
)
const search = useFilesSearch(
  source,
  path,
  searchQuery,
  filter,
  prefs.showHidden,
)

function onSearch(query: string) {
  router.setQuery({ q: query.trim() || null })
}

function onFilter(next: FilesFilter) {
  router.setQuery(formatFilter(next))
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

onMounted(() => {
  openFrames += 1
  void prefs.ensureLoaded()
})
// FR-016: the media server URLs of this tab end with it.
const wmTab = useWmTab()
const { tabId } = wmTab

onBeforeUnmount(() => {
  void releaseTabAsync(tabId).catch(() => {})
  openFrames -= 1
  if (openFrames > 0) return
  clearFilesThumbnails()
  clearFileIcons()
})

/** Opens a folder, with the filter kept and the search ended. */
function go(target: string, open?: string) {
  const { t: types, s: size, d: date } = place.value ?? {}
  router.push(
    filesLocation({
      source: source.value,
      path: target,
      open,
      t: types,
      s: size,
      d: date,
    }),
  )
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
  tabId,
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
  router.setQuery({ open: entry.name }, { push: true })
}

/** The file in the viewer: its entry once the folder is listed, until then only name and path (a
 * restored tab opens the file before its folder arrives). */
const openEntry = computed(
  (): (Pick<Entry, 'name' | 'path'> & Partial<Entry>) | null => {
    const name = place.value?.open
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
  router.setQuery({ open: null })
}

function showSibling(entry: Entry) {
  router.setQuery({ open: entry.name })
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
const sidebar = useTemplateRef<HTMLElement>('sidebar')
const { wideHidden, menuOpen, visible, toggle, close } = useSidebarFrame(
  root,
  () => sidebar.value,
)

const sidebarClass = computed(() => [
  'absolute inset-0 z-20 bg-background transition-[translate,width,visibility] duration-200 ease-out motion-reduce:transition-none',
  '@2xl:static @2xl:z-auto @2xl:shrink-0 @2xl:overflow-hidden @2xl:translate-x-0',
  menuOpen.value ? 'visible translate-x-0' : 'invisible -translate-x-full',
  wideHidden.value ? '@2xl:invisible @2xl:w-0' : '@2xl:visible @2xl:w-60',
])

function onSidebarPick(target: string) {
  close()
  router.push(filesLocation({ source: { kind: 'device' }, path: target }))
}

function onPickStorage(storageId: string) {
  close()
  router.push(
    filesLocation({
      source: { kind: 'storage', storageId },
      // Another own device opens at its home folder (the start resolves it).
      path: isPeerStorage(storageId) ? null : '/',
    }),
  )
}

// --- Storages (US5) ---

const storageId = computed(() =>
  source.value.kind === 'storage' ? source.value.storageId : null,
)
const storageName = computed(
  () =>
    sources.state.value?.storages.find(
      (storage) => storage.id === storageId.value,
    )?.name ??
    sources.state.value?.devices.find((device) => device.id === storageId.value)
      ?.name ??
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

const { openApp } = useWmTab()
function openStorageSettings() {
  openApp('system.settings', '/storage')
}

// A storage is not watched: it loads again when its tab comes to the front.
const wm = useWindowManagerStore()
const tabActive = computed(
  () =>
    wm.windows.find((window) => window.id === wmTab.windowId)?.activeTabId ===
    tabId,
)
watch(tabActive, (active) => {
  if (!active) return
  void sources.execute()
  if (storageId.value !== null) void folder.reload(true)
})
</script>

<template>
  <div
    ref="root"
    class="@container flex h-full min-h-0 flex-col bg-muted/20"
    data-files-app
    @keydown="onKeydown"
  >
    <FilesToolbar
      class="h-13 shrink-0 px-2"
      :path="path"
      :root-label="storageName"
      :sidebar-visible="visible"
      :can-go-up="!!path && parentPath(path) !== null"
      @toggle-sidebar="toggle"
      @go="go"
      @up="goUp"
      @reload="folder.reload()"
    />
    <FilesSearchBar
      :query="searchQuery"
      :filter="filter"
      @search="onSearch"
      @filter="onFilter"
    />
    <div class="relative flex min-h-0 flex-1 overflow-hidden">
      <nav
        id="files-sidebar"
        ref="sidebar"
        :class="sidebarClass"
        :aria-label="t('files.sidebar.label')"
        data-testid="files-sidebar"
      >
        <FilesSidebar
          :sources="sources.state.value"
          :current="storageId === null ? path : null"
          :current-storage="storageId"
          @pick="onSidebarPick"
          @pick-storage="onPickStorage"
          @add-storage="openStorageSettings"
          @query-contact="queryContactAsync"
        />
      </nav>
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
              :dimmed="dimmed"
              :settings-link="settingsLink"
              @press="onPress"
              @open="activate"
              @clear="clearSelection()"
              @context="onContext"
              @drop="onDrop"
              @settings="openStorageSettings"
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
        <FilesViewer
          v-if="openEntry"
          :source="source"
          :entry="openEntry"
          :siblings="shown"
          @close="closeViewer"
          @show="showSibling"
          @open-system="openSystemAsync"
        />
      </main>
    </div>
    <FilesTransferBar :tab-id="tabId" />
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
      :contacts="sources.state.value?.contacts ?? []"
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
