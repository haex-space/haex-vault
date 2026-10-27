<script lang="ts">
import { isPeerStorage } from '~/lib/files/registry'

/** How many file browser frames are open, across all windows of the process. */
let openFrames = 0
</script>

<script setup lang="ts">
/**
 * The file browser frame (spec 044, US1), laid out like the password manager: a toolbar, below it
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
const { queryContactAsync } = useFilesContactQuery(sources)

const root = useTemplateRef<HTMLElement>('root')
const sidebar = useTemplateRef<HTMLElement>('sidebar')
const pathHost = useTemplateRef<HTMLElement>('pathHost')
const viewerHost = useTemplateRef<HTMLElement>('viewerHost')

const { place, go, tabId, tabActive, openStorageSettings, onKeydown } =
  useFilesSplit({
    sources: sources.state,
    reloadSources: () => void sources.execute(),
    pathHost,
    viewerHost,
  })

onMounted(() => {
  openFrames += 1
  void prefs.ensureLoaded()
})

// FR-016: the media server URLs of this tab end with its id.
onBeforeUnmount(() => {
  void releaseTabAsync(tabId).catch(() => {})
  openFrames -= 1
  if (openFrames > 0) return
  clearFilesThumbnails()
  clearFileIcons()
})

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
  go({ source: { kind: 'device' }, path: target })
}

function onPickStorage(storageId: string) {
  close()
  go({
    source: { kind: 'storage', storageId },
    // Another own device opens at its home folder (the start resolves it).
    path: isPeerStorage(storageId) ? null : '/',
  })
}

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
    <FilesToolbar
      class="h-13 shrink-0 px-2"
      :sidebar-visible="visible"
      @toggle-sidebar="toggle"
    >
      <!-- Where the panel puts its path bar. -->
      <div ref="pathHost" class="contents" />
    </FilesToolbar>
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
          :current="storageId === null ? place.path : null"
          :current-storage="storageId"
          @pick="onSidebarPick"
          @pick-storage="onPickStorage"
          @add-storage="openStorageSettings"
          @query-contact="queryContactAsync"
        />
      </nav>
      <div class="relative flex min-w-0 flex-1">
        <FilesPanel side="left" />
        <!-- The viewer of a panel covers the whole panel area. -->
        <div ref="viewerHost" class="contents" />
      </div>
    </div>
    <FilesTransferBar :tab-id="tabId" />
  </div>
</template>
