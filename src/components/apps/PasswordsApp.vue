<script lang="ts">
import { headerControl, inWindowHeader } from '~/lib/wm/headerDom'

/** How many password manager frames are open, across all windows of the process. */
let openFrames = 0
</script>

<script setup lang="ts">
/**
 * The password manager frame (spec 034-password-manager, research R13), laid out like the chat and
 * settings frames: the sidebar button and the controls are in the window's header (spec 093), below
 * it the sidebar and the content.
 * The frame is a size container, so the sidebar follows the window's width, not the screen's: from
 * `@2xl` it sits beside the content and can be hidden, below it is gone and opens over the content
 * (`useSidebarFrame`). Every place is a tab location (spec 020); only ids go into the path.
 */
const { t } = useI18n()

const store = usePasswordsStore()
onMounted(() => {
  void store.reloadAsync()
})

// Spec 036 (FR-021): the Ablage is shared by every window of the password manager and dropped when
// the last one closes.
const clipboard = usePasswordsClipboardStore()
onMounted(() => {
  openFrames += 1
})
onBeforeUnmount(() => {
  openFrames -= 1
  if (openFrames > 0) return
  clipboard.clear()
  // Spec 036, research R14: the thumbnails go with the last frame.
  clearPasswordsThumbnails()
})

// Spec 036 (FR-016): the list shortcuts act with the focus anywhere in this window.
const { onKeydown } = providePasswordsShortcuts()

// The header is the window's, outside this root: its keys (Ctrl+A, Ctrl+V, ... with the focus on the
// sidebar button or "+") reach the same handler, as they did when the row was inside the root. The
// field of the search and open menus keep theirs: the resolver leaves them out. Every tab of the
// window stays mounted, so only the shown one takes them.
const wm = useWindowManagerStore()
const wmTab = useWmTab()
const tabActive = computed(
  () =>
    wm.windows.find((window) => window.id === wmTab.windowId)?.activeTabId ===
    wmTab.tabId,
)
useEventListener(document, 'keydown', (event: KeyboardEvent) => {
  if (!tabActive.value) return
  const target = event.target instanceof Element ? event.target : null
  if (inWindowHeader(target, wmTab.windowId)) onKeydown(event)
})

const root = useTemplateRef<HTMLElement>('root')
const sidebar = useTemplateRef<HTMLElement>('sidebar')

const { wideHidden, menuOpen, visible, overlaying, toggle, close } =
  useSidebarFrame(root, () => sidebar.value)

// The first row is the window's header (spec 093-app-headers, US1); the dialogs of its controls are
// mounted here, not in a control, so they stay open when the control is collapsed (FR-009).
const { settingsOpen, tagManagerOpen, folderDialogOpen, openFolderId } =
  usePasswordsHeader(() => visible.value, toggle)

const sidebarClass = computed(() => [
  'absolute inset-0 z-20 bg-background transition-[translate,width,visibility] duration-200 ease-out motion-reduce:transition-none',
  '@2xl:static @2xl:z-auto @2xl:shrink-0 @2xl:overflow-hidden @2xl:translate-x-0',
  menuOpen.value ? 'visible translate-x-0' : 'invisible -translate-x-full',
  wideHidden.value ? '@2xl:invisible @2xl:w-0' : '@2xl:visible @2xl:w-64',
])

function onEscape() {
  if (!overlaying.value) return
  close()
  headerControl(root.value, 'passwords-sidebar-toggle')?.focus()
}
</script>

<template>
  <div
    ref="root"
    class="@container relative flex h-full min-h-0 flex-col bg-muted/20"
    data-passwords-app
    @keydown="onKeydown"
  >
    <div class="relative flex min-h-0 flex-1 overflow-hidden">
      <nav
        id="passwords-sidebar"
        ref="sidebar"
        :class="sidebarClass"
        :aria-label="t('passwords.title')"
        data-testid="passwords-sidebar"
        @keydown.esc="onEscape"
      >
        <PasswordsSidebar @navigated="close" />
      </nav>
      <main
        class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto"
        :inert="overlaying || undefined"
      >
        <WmRouterView />
      </main>
    </div>
    <PasswordsClipboardSetting v-model:open="settingsOpen" />
    <PasswordsTagManager v-model:open="tagManagerOpen" />
    <PasswordsFolderDialog
      v-model:open="folderDialogOpen"
      :group="null"
      :parent-id="openFolderId"
    />
  </div>
</template>
