<script lang="ts">
import { sidebarPanelClass } from '~/lib/wm/slide'
import type { Drive } from '@bindings/Drive'
import type { DriveOpened } from '@bindings/DriveOpened'
import type { FilesPin } from '~/lib/files/pins'
import { isPeerStorage } from '~/lib/files/registry'
import {
  filesHeader,
  filesHeaderCommand,
  inWindowHeader,
} from '~/lib/files/header'
import { splitAvailable } from '~/lib/files/split'

/** How many file browser frames are open, across all windows of the process. */
let openFrames = 0
</script>

<script setup lang="ts">
/**
 * The file browser frame (spec 044, US1): its toolbar is the window's header (spec 090), below it
 * the sidebar (known places, drives) and the panel area with the open folder as list or grid and
 * the viewer over it. Where the tab stands is its location (`lib/files/registry.ts`), so moving
 * between folders keeps this component mounted and session restore (spec 022) brings tab and open
 * file back. The folder itself is `components/files/Panel.vue` (spec 062); the frame owns what the
 * panels share (`composables/useFilesSplit.ts`): the tab location, the sources, the sidebar and the
 * transfer bar.
 */

const { t } = useI18n()
const prefs = useFilesPrefs()
const { sourcesAsync, releaseTabAsync } = useFiles()

const sources = useAsyncState(() => sourcesAsync(), null)
// Spec 064: the drive list stays live while a frame is open; until its first message the sources'
// drives stand in.
const liveDrives = useFilesDrives()
const drives = computed(() =>
  liveDrives.received.value
    ? liveDrives.drives.value
    : (sources.state.value?.drives ?? []),
)
const { nameOf, openDriveAsync, releaseDriveAsync } = useFilesDriveActions()
const { queryContactAsync } = useFilesContactQuery(sources)

const root = useTemplateRef<HTMLElement>('root')
const sidebar = useTemplateRef<HTMLElement>('sidebar')
const area = useTemplateRef<HTMLElement>('area')
const viewerHost = useTemplateRef<HTMLElement>('viewerHost')

// Two panels: not on a phone (FR-021; until the platform is known, and in a browser preview, yes), and
// only while the panel area is wide enough (FR-022).
const { capabilities } = useDeviceCapabilities()
const available = computed(() => splitAvailable(capabilities.value?.platform))
const fits = useSplitFit(area)

const {
  place,
  go,
  tabId,
  tabActive,
  openStorageSettings,
  onKeydown,
  toggle: toggleSplit,
  on: splitOn,
  shown: split,
  activeSide,
  hasOther,
  activeHeader,
} = useFilesSplit({
  sources: sources.state,
  reloadSources: () => void sources.execute(),
  drives,
  drivesKnown: computed(
    () => liveDrives.received.value || sources.state.value !== null,
  ),
  viewerHost,
  available,
  fits,
})

/** The panels that are mounted: the active one, and the other once it has a place (it stays mounted
 * and hidden while the split is off, so its folder and selection stay). */
const panels = computed(() =>
  (['left', 'right'] as const).filter(
    (side) => side === activeSide.value || hasOther.value,
  ),
)
const visiblePanel = (side: 'left' | 'right') =>
  split.value || side === activeSide.value

onMounted(() => {
  openFrames += 1
  if (openFrames === 1) liveDrives.start()
  void prefs.ensureLoaded()
})

// FR-016: the media server URLs of this tab end with its id.
onBeforeUnmount(() => {
  void releaseTabAsync(tabId).catch(() => {})
  openFrames -= 1
  if (openFrames > 0) return
  liveDrives.stop()
  clearFilesThumbnails()
  clearFileIcons()
})

const { wideHidden, menuOpen, visible, toggle, close } = useSidebarFrame(
  root,
  () => sidebar.value,
)

// The first row of the file browser is in the window's header (spec 090 US4): with one panel the
// whole bar of that panel (`lib/files/header.ts`), with two only the sidebar button and the switch,
// as each panel then has its own bar.
const barState = computed(() =>
  split.value ? null : (activeHeader.value?.state.value ?? null),
)
useWmHeader(
  () =>
    filesHeader(
      {
        sidebarVisible: visible.value,
        splitAvailable: available.value,
        splitOn: splitOn.value,
        splitDisabled: !fits.value && !splitOn.value,
      },
      barState.value,
      t,
    ),
  (event) => {
    const command = filesHeaderCommand(event, barState.value)
    if (!command) return
    if (command.kind === 'sidebar') toggle()
    else if (command.kind === 'split') toggleSplit()
    else activeHeader.value?.run(command)
  },
)

// The header is the window's, outside this frame's root: its keys (Ctrl+F, Ctrl+R, Ctrl+Shift+N, ...
// with the focus on one of its controls) reach the same handler as the keys of the sidebar. A text
// field of the header (the path being edited) and open menus keep theirs: the panel's rules
// (`resolveFilesShortcut`) leave them out.
const wmTab = useWmTab()
useEventListener(document, 'keydown', (event: KeyboardEvent) => {
  if (!tabActive.value) return
  const target = event.target instanceof Element ? event.target : null
  if (inWindowHeader(target, wmTab.windowId)) onKeydown(event)
})

const sidebarClass = computed(() =>
  sidebarPanelClass(menuOpen.value, wideHidden.value, '@2xl:w-60'),
)

function onSidebarPick(target: string) {
  close()
  go({ source: { kind: 'device' }, path: target })
}

/**
 * A drive opens once it is mounted or unlocked (spec 064 FR-006, FR-013); a locked one without a
 * saved password that works asks for it. A failure leaves the tab where it is.
 */
async function onPickDrive(drive: Drive) {
  const opened = await openDriveAsync(drive)
  if (opened?.kind === 'opened') onSidebarPick(opened.path)
  else if (opened?.kind === 'needsPassword') unlocking.value = { drive, opened }
}

/** A locked drive waiting for its password in the unlock dialog (contracts/ui.md). */
const unlocking = ref<{
  drive: Drive
  opened: Extract<DriveOpened, { kind: 'needsPassword' }>
} | null>(null)
const unlockOpen = computed({
  get: () => unlocking.value !== null,
  set: (open: boolean) => {
    if (!open) unlocking.value = null
  },
})

/** A drive with transfers still running on it, waiting for the yes to cancel them (contracts/ui.md). */
const releasing = ref<{ drive: Drive; count: number } | null>(null)
const releaseOpen = computed({
  get: () => releasing.value !== null,
  set: (open: boolean) => {
    if (!open) releasing.value = null
  },
})

async function onReleaseDrive(drive: Drive) {
  const count = await releaseDriveAsync(drive, false)
  if (count !== null) releasing.value = { drive, count }
}

async function onReleaseConfirmed() {
  const pending = releasing.value
  releasing.value = null
  if (pending) await releaseDriveAsync(pending.drive, true)
}

/** The sync rules take the panel; going back returns to the folder it showed (spec 061). */
function onPickSyncRules() {
  close()
  go({ source: place.value.source, path: place.value.path, sr: '1' })
}

/** The list of the vault's own file shares takes the panel, like the sync rules (spec 071). */
function onPickShares() {
  close()
  go({ source: place.value.source, path: place.value.path, sh: '1' })
}

/** A pinned folder opens like a place (spec 071). */
function onPickPin(pin: FilesPin) {
  close()
  go({ source: pin.source, path: pin.path })
}

function onPickStorage(storageId: string) {
  close()
  go({
    source: { kind: 'storage', storageId },
    // Another own device opens at its home folder (the start resolves it).
    path: isPeerStorage(storageId) ? null : '/',
  })
}

/** A list (sync rules, file shares) shows in place of the folder. */
const listView = computed(() => !!place.value.sr || !!place.value.sh)

const storageId = computed(() =>
  place.value.source.kind === 'storage' ? place.value.source.storageId : null,
)

// A storage is not watched: its sources load again when its tab comes to the front.
watch(tabActive, (active) => {
  if (active) void sources.execute()
})
</script>

<template>
  <div
    ref="root"
    class="@container flex h-full min-h-0 flex-col bg-muted/20"
    data-files-app
    @keydown="onKeydown"
  >
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
          :drives="drives"
          :busy="liveDrives.busy.value"
          :current="storageId === null && !listView ? place.path : null"
          :current-storage="listView ? null : storageId"
          :current-path="listView ? null : place.path"
          :sync-rules-current="!!place.sr"
          :shares-current="!!place.sh && !place.sr"
          @pick="onSidebarPick"
          @pick-sync-rules="onPickSyncRules"
          @pick-shares="onPickShares"
          @pick-pin="onPickPin"
          @pick-drive="onPickDrive"
          @release-drive="onReleaseDrive"
          @pick-storage="onPickStorage"
          @add-storage="openStorageSettings"
          @query-contact="queryContactAsync"
        />
      </nav>
      <div
        ref="area"
        :class="[
          'relative flex min-h-0 min-w-0 flex-1',
          split && 'gap-1.5 p-1.5',
        ]"
      >
        <FilesPanel
          v-for="side in panels"
          v-show="visiblePanel(side)"
          :key="side"
          :side="side"
        />
        <!-- The viewer of a panel covers the whole panel area. -->
        <div ref="viewerHost" class="contents" />
      </div>
    </div>
    <FilesTransferBar :tab-id="tabId" />
    <FilesDriveReleaseDialog
      v-model:open="releaseOpen"
      :count="releasing?.count ?? 0"
      :name="releasing ? nameOf(releasing.drive) : ''"
      :eject="releasing?.drive.release !== 'unmount'"
      @confirm="onReleaseConfirmed"
    />
    <FilesDriveUnlockDialog
      v-model:open="unlockOpen"
      :drive="unlocking?.drive ?? null"
      :reason="unlocking?.opened.reason ?? 'none'"
      :message="unlocking?.opened.message ?? null"
      :matches="unlocking?.opened.candidates ?? []"
      @opened="onSidebarPick"
    />
  </div>
</template>
