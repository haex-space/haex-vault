<script setup lang="ts">
/**
 * The address book frame (spec 051, FR-002), laid out like the password manager: a toolbar row
 * with the sidebar button, below it the sidebar with all contacts and the groups, and the content.
 * The frame is a size container, so the sidebar follows the window's width: from `@2xl` it sits
 * beside the content and can be hidden, below it opens over the content (`useSidebarFrame`). Every
 * place is a tab location (spec 020); only ids go into the path.
 */
const { t } = useI18n()

const store = useContactsStore()
onMounted(() => {
  void store.reloadAsync()
})

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
    ?.querySelector<HTMLElement>('[data-testid="contacts-sidebar-toggle"]')
    ?.focus()
}
</script>

<template>
  <div
    ref="root"
    class="@container flex h-full min-h-0 flex-col bg-muted/20"
    data-contacts-app
  >
    <ContactsToolbar
      class="h-13 shrink-0 px-2"
      :sidebar-visible="visible"
      @toggle-sidebar="toggle"
    />
    <div class="relative flex min-h-0 flex-1 overflow-hidden">
      <nav
        id="contacts-sidebar"
        ref="sidebar"
        :class="sidebarClass"
        :aria-label="t('contacts.title')"
        data-testid="contacts-sidebar"
        @keydown.esc="onEscape"
      >
        <ContactsSidebar @navigated="close" />
      </nav>
      <main
        class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto"
        :inert="overlaying || undefined"
      >
        <p
          v-if="store.lastError"
          class="m-3 rounded-lg border border-destructive/40 p-3 text-sm text-destructive"
          role="alert"
        >
          {{ store.lastError }}
        </p>
        <WmRouterView />
      </main>
    </div>
  </div>
</template>
