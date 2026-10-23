<script lang="ts">
/** How many password manager frames are open, across all windows of the process. */
let openFrames = 0
</script>

<script setup lang="ts">
/**
 * The password manager frame (spec 034-password-manager, research R13), laid out like the chat and
 * settings frames: a toolbar row with the sidebar button, below it the sidebar and the content.
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

const root = useTemplateRef<HTMLElement>('root')
const sidebar = useTemplateRef<HTMLElement>('sidebar')

const { wideHidden, menuOpen, visible, overlaying, toggle, close } =
  useSidebarFrame(root, () => sidebar.value)

const sidebarClass = computed(() => [
  'absolute inset-0 z-20 bg-background transition-[translate,width,visibility] duration-200 ease-out motion-reduce:transition-none',
  '@2xl:static @2xl:z-auto @2xl:shrink-0 @2xl:overflow-hidden @2xl:translate-x-0',
  menuOpen.value ? 'visible translate-x-0' : 'invisible -translate-x-full',
  wideHidden.value ? '@2xl:invisible @2xl:w-0' : '@2xl:visible @2xl:w-64',
])

function onEscape() {
  if (!overlaying.value) return
  close()
  root.value
    ?.querySelector<HTMLElement>('[data-testid="passwords-sidebar-toggle"]')
    ?.focus()
}
</script>

<template>
  <div
    ref="root"
    class="@container relative flex h-full min-h-0 flex-col bg-muted/20"
    data-passwords-app
    @keydown="onKeydown"
  >
    <PasswordsToolbar
      class="h-13 shrink-0 px-2"
      :sidebar-visible="visible"
      @toggle-sidebar="toggle"
    />
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
  </div>
</template>
