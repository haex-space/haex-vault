<script lang="ts">
import { toast } from 'vue-sonner'
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import type { TransferOp } from '@bindings/TransferOp'
import { asFilesError } from '~/composables/useFiles'
import {
  clickEntry,
  type ClickKeys,
  type DragPayload,
  dropOp,
  EMPTY_SELECTION,
  type FilesClipboard,
  menuSelection,
  pasteRefusal,
  pruneSelection,
  type Selection,
} from '~/lib/files/clipboard'
import {
  admits,
  type FilesFilter,
  formatFilter,
  isFiltering,
  parseFilter,
} from '~/lib/files/filters'
import { asksBeforeDelete, canCreateEncrypted } from '~/lib/files/encrypted'
import { entriesMenu, type FilesCommand, folderMenu } from '~/lib/files/menus'
import { filesLocation, parseFilesPlace } from '~/lib/files/registry'
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
 * Managing files (US3): selection, the context menu, holzi's clipboard, dragging inside holzi and
 * from the system, and the transfers they start (`stores/filesTransfers.ts`).
 */

const { t } = useI18n()
const router = useTabRouter()
const prefs = useFilesPrefs()
const {
  sourcesAsync,
  releaseTabAsync,
  statAsync,
  encryptedNoticeShownAsync,
  encryptedNoticeAckAsync,
} = useFiles()
const transfers = useFilesTransfersStore()
const { isAndroid } = useDeviceCapabilities()

const place = computed(() => parseFilesPlace(router.route))
const source = computed(
  () => place.value?.source ?? { kind: 'device' as const },
)
const path = computed(() => place.value?.path ?? null)

const sources = useAsyncState(() => sourcesAsync(), null)

// The start (no path yet) opens the home folder, or the first drive where there is none.
watchEffect(() => {
  if (path.value !== null) return
  // A storage starts at its root.
  if (source.value.kind === 'storage') {
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

const folder = useFilesFolder(source, path)
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

const selection = ref<Selection>(EMPTY_SELECTION)
const selecting = computed(() => selection.value.paths.length > 0)
watch(path, () => (selection.value = EMPTY_SELECTION))
watch(shown, (entries) => {
  selection.value = pruneSelection(
    selection.value,
    entries.map((entry) => entry.path),
  )
})

/** The open folder is one of holzi's own places (FR-037): nothing changes in it. */
const folderOwned = ref(false)
/** The open folder is, or lies in, an encrypted folder (spec 048). */
const folderEncrypted = ref(false)
/** Foreign files in the open encrypted folder (spec 048 FR-048); 0 below its top. */
const folderForeign = ref(0)
watch(
  [source, path],
  async ([current, folderPath]) => {
    folderOwned.value = false
    folderEncrypted.value = false
    folderForeign.value = 0
    if (folderPath === null) return
    const entry = await statAsync(current, folderPath).catch(() => null)
    if (path.value === folderPath) {
      folderOwned.value = entry?.holziOwned ?? false
      folderEncrypted.value = !!entry?.encrypted
      folderForeign.value = entry?.encrypted?.foreign ?? 0
    }
  },
  { immediate: true },
)

const entryAt = (target: string) =>
  shown.value.find((entry) => entry.path === target)

function onPress(entry: Entry, keys: ClickKeys) {
  const result = clickEntry(
    selection.value,
    entry.path,
    shown.value.map((candidate) => candidate.path),
    keys,
  )
  selection.value = result.selection
  if (result.activate) activate(entry)
}

/** What the open context menu acts on: entries, or the folder's empty area (`null`). */
const menuTargets = ref<Selection | null>(null)

function onContext(entry: Entry | null) {
  menuTargets.value = entry ? menuSelection(selection.value, entry.path) : null
}

const canPaste = computed(() => transfers.clipboard !== null && !!path.value)

function entriesOf(targets: Selection): Entry[] {
  return targets.paths.flatMap((target) => entryAt(target) ?? [])
}

const menuEntries = computed(() => {
  const targets = menuTargets.value
  if (!targets)
    return folderMenu({
      readOnly: folderOwned.value,
      canPaste: canPaste.value,
      canEncrypt: canCreateEncrypted({
        source: source.value,
        path: path.value,
        inEncrypted: folderEncrypted.value,
        owned: folderOwned.value,
      }),
    })
  const entries = entriesOf(targets)
  return entriesMenu({
    count: entries.length,
    singleFile: entries.length === 1 && entries[0]?.kind === 'file',
    readOnly: folderOwned.value || entries.some((entry) => entry.holziOwned),
    selecting: selecting.value,
    systemApps: !isAndroid.value,
  })
})

const dimmed = computed(() =>
  transfers.clipboard?.op === 'cut'
    ? transfers.clipboard.items.map((item) => item.path)
    : [],
)

function errorText(cause: unknown): string {
  const code = asFilesError(cause)?.code
  return code ? t(`files.error.${code}`) : String(cause)
}

/** Starts a transfer; a refusal shows as a message. Whether it started. */
async function startAsync(
  op: TransferOp,
  from: SourceRef,
  paths: string[],
  to: string | null,
): Promise<boolean> {
  try {
    await transfers.startAsync(
      op,
      from,
      paths,
      to === null ? null : { source: source.value, path: to },
      tabId,
      // A storage is not watched: its folder loads again once the transfer is through.
      () => void folder.reload(),
    )
    return true
  } catch (cause) {
    toast.error(errorText(cause))
    return false
  }
}

/**
 * Copies or moves `clip` into the folder `target` (paste, or a drop inside holzi). Whether the
 * transfer started.
 */
async function placeAsync(
  clip: FilesClipboard,
  target: string,
  targetOwned: boolean,
): Promise<boolean> {
  const refusal = pasteRefusal(clip.op, clip.folder, clip.items, {
    path: target,
    holziOwned: targetOwned,
  })
  if (refusal === 'nothing') return false
  if (refusal) {
    toast.error(t(`files.error.${refusal}`))
    return false
  }
  return await startAsync(
    clip.op === 'cut' ? 'move' : 'copy',
    clip.source as SourceRef,
    clip.items.map((item) => item.path),
    target,
  )
}

const naming = ref(false)
const renaming = ref<Entry | null>(null)
/** The name dialog creates an encrypted folder (spec 048). */
const namingEncrypted = ref(false)
/** The notice before the first encrypted folder (FR-002) is open. */
const noticing = ref(false)

function startEncrypted() {
  namingEncrypted.value = true
  naming.value = true
}

async function ackNotice() {
  await encryptedNoticeAckAsync().catch(() => {})
  startEncrypted()
}
const deleting = ref(false)
const pendingDelete = ref<string[]>([])
/** Foreign files that go with the encrypted folders of `pendingDelete` (spec 048 FR-048). */
const pendingForeign = ref(0)

/** Asks before deleting `entries` for good, naming the foreign files of encrypted folders. */
async function askDeleteAsync(entries: Entry[]) {
  const folders = entries.filter((entry) => entry.encrypted?.role === 'folder')
  const stats = await Promise.all(
    folders.map((entry) =>
      statAsync(source.value, entry.path).catch(() => null),
    ),
  )
  pendingForeign.value = stats.reduce(
    (sum, entry) => sum + (entry?.encrypted?.foreign ?? 0),
    0,
  )
  deleting.value = true
}

const {
  noticing: openNoticing,
  openAsync: openSystemAsync,
  confirmAsync: confirmOpenAsync,
} = useFilesOpenSystem(source, (cause) => toast.error(errorText(cause)))

function remember(op: FilesClipboard['op'], entries: Entry[]) {
  if (!path.value || !entries.length) return
  transfers.clipboard = {
    op,
    source: source.value,
    folder: path.value,
    items: entries.map((entry) => ({ path: entry.path, kind: entry.kind })),
  }
  selection.value = EMPTY_SELECTION
}

function deleteNow() {
  const paths = pendingDelete.value
  pendingDelete.value = []
  selection.value = EMPTY_SELECTION
  void startAsync('delete', source.value, paths, null)
}

/** Runs a command on `targets` (the context menu's, or the selection for the action bar). */
async function run(command: FilesCommand, targets: Selection | null) {
  const entries = targets ? entriesOf(targets) : []
  const [first] = entries
  switch (command) {
    case 'open':
      if (first) activate(first)
      break
    case 'openSystem':
      if (first) await openSystemAsync(first.path)
      break
    case 'select':
      selection.value = {
        paths: [
          ...new Set([...selection.value.paths, ...(targets?.paths ?? [])]),
        ],
        anchor: targets?.anchor ?? null,
      }
      break
    case 'copy':
    case 'cut':
      remember(command, entries)
      break
    case 'paste': {
      const clip = transfers.clipboard
      if (!clip || !path.value) break
      // A refused cut stays on the clipboard, so it can go elsewhere.
      const started = await placeAsync(clip, path.value, folderOwned.value)
      if (started && clip.op === 'cut') transfers.clipboard = null
      break
    }
    case 'rename':
      if (first) {
        renaming.value = first
        naming.value = true
      }
      break
    case 'newFolder':
      renaming.value = null
      namingEncrypted.value = false
      naming.value = true
      break
    case 'newEncryptedFolder':
      renaming.value = null
      // Before the first encrypted folder on this device, the notice of FR-002.
      if (await encryptedNoticeShownAsync().catch(() => false)) startEncrypted()
      else noticing.value = true
      break
    case 'delete':
      pendingDelete.value = entries.map((entry) => entry.path)
      // Desktops move to the trash; Android, storages and encrypted folders delete for good and
      // ask first (FR-023, spec 048 FR-046).
      if (
        asksBeforeDelete({
          source: source.value,
          android: isAndroid.value,
          inEncrypted: folderEncrypted.value,
          entries,
        })
      )
        await askDeleteAsync(entries)
      else deleteNow()
      break
  }
}

function runMenu(command: FilesCommand) {
  void run(command, menuTargets.value)
}

function runBar(command: FilesCommand) {
  void run(command, selection.value)
}

function onDrop(
  payload: DragPayload,
  target: string,
  keys: { ctrl: boolean; alt: boolean },
) {
  // Dropped onto itself.
  if (payload.items.some((item) => item.path === target)) return
  const sameSource =
    JSON.stringify(payload.source) === JSON.stringify(source.value)
  const op = dropOp(sameSource, {
    ...keys,
    mac: /Mac/.test(navigator.userAgent),
  })
  const owned =
    target === path.value
      ? folderOwned.value
      : (entryAt(target)?.holziOwned ?? false)
  selection.value = EMPTY_SELECTION
  void placeAsync({ ...payload, op }, target, owned)
}

async function importDropped(paths: string[]) {
  if (!path.value) return
  try {
    await transfers.importAsync(
      paths,
      { source: source.value, path: path.value },
      tabId,
    )
  } catch (cause) {
    toast.error(errorText(cause))
  }
}

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
    filesLocation({ source: { kind: 'storage', storageId }, path: '/' }),
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
    )?.name,
)

/** A storage's credentials or host key failed: its settings can fix them (spec 038, 050). */
const settingsLink = computed(
  () =>
    storageId.value !== null &&
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
  if (active && storageId.value !== null) void folder.reload()
})
</script>

<template>
  <div
    ref="root"
    class="@container flex h-full min-h-0 flex-col bg-muted/20"
    data-files-app
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
        />
      </nav>
      <main
        class="relative flex min-w-0 flex-1 flex-col"
        data-testid="files-main"
        @keydown.esc="selection = EMPTY_SELECTION"
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
          :disabled="folderOwned || !path"
          @drop="importDropped"
        >
          <FilesMenu :entries="menuEntries" @run="runMenu">
            <FilesFolderView
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
              @clear="selection = EMPTY_SELECTION"
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
          :read-only="folderOwned"
          @run="runBar"
          @clear="selection = EMPTY_SELECTION"
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
