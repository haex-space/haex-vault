<script setup lang="ts">
/**
 * The chat frame (spec 076): the thread sidebar beside the content, with no toolbar row of its own.
 * The sidebar is never gone (FR-001): wide, it is expanded or collapsed to a rail of avatars; narrow,
 * the rail stays and the toggle opens the full sidebar over the content. Like the settings frame
 * (`apps/SettingsApp.vue`) it is a size container, so this follows the window's width, not the
 * screen's. The width logic is `useSidebarFrame`, unchanged: it learns "wide" from the CSS position
 * of the probe below, so the `@2xl` threshold lives only in the classes. The `sidebar` slot gets
 * `run`, to wrap what a pick in the overlay does, and what the sidebar's own toggle needs.
 */
const { t } = useI18n()
const sidebarStore = useChatSidebarStore()

const frame = useTemplateRef<HTMLElement>('frame')
const probe = useTemplateRef<HTMLElement>('probe')

const { wideHidden, menuOpen, wide, visible, overlaying, toggle, close } =
  useSidebarFrame(frame, () => probe.value)

// FR-008: the collapsed state outlives the tab; the overlay does not (data-model.md).
wideHidden.value = sidebarStore.collapsed
watch(wideHidden, (hidden) => (sidebarStore.collapsed = hidden))

const toggleLabel = computed(() =>
  visible.value ? t('chat.sidebar.hide') : t('chat.sidebar.show'),
)

const state = computed(() => {
  if (wide.value) return visible.value ? 'expanded' : 'rail'
  return menuOpen.value ? 'overlay' : 'rail'
})

const panelClass = computed(() => [
  'h-full shrink-0 overflow-hidden bg-background transition-[width] duration-200 motion-reduce:transition-none',
  menuOpen.value
    ? 'absolute inset-y-0 left-0 z-20 w-64 @2xl:static @2xl:z-auto'
    : 'w-14',
  wideHidden.value ? '@2xl:w-14' : '@2xl:w-64',
])

/** For what is picked in the sidebar: the overlay closes, back to the content; a wide window keeps
 * its sidebar. */
function run(action: () => void) {
  close()
  action()
}

function onEscape() {
  if (!overlaying.value) return
  close()
  frame.value
    ?.querySelector<HTMLElement>('[data-testid="chat-sidebar-toggle"]')
    ?.focus()
}
</script>

<template>
  <main ref="frame" class="@container relative flex h-full min-h-0 bg-muted/20">
    <span
      ref="probe"
      class="pointer-events-none invisible absolute size-0 @2xl:static"
      aria-hidden="true"
    />
    <!-- The rail keeps its place while the full sidebar lies over the content. -->
    <div v-if="menuOpen" class="w-14 shrink-0 @2xl:hidden" aria-hidden="true" />
    <div
      id="chat-sidebar"
      :data-state="state"
      :class="panelClass"
      @keydown.esc="onEscape"
    >
      <slot
        name="sidebar"
        :run="run"
        :collapsed="!visible"
        :toggle="toggle"
        :toggle-label="toggleLabel"
        :toggle-expanded="visible"
      />
    </div>
    <div
      class="flex min-h-0 min-w-0 flex-1 flex-col"
      :inert="overlaying || undefined"
    >
      <slot />
    </div>
    <slot name="dialogs" />
  </main>
</template>
