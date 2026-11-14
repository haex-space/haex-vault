<script setup lang="ts">
/**
 * The chat frame (spec 076): the thread sidebar beside the content, with no toolbar row of its own.
 * Like every app sidebar it can be hidden completely (`WmSidebar`): the button is in the window's
 * header. Wide, it sits beside the content; narrow, it opens over it, and the button and Escape
 * close it again. Like the settings frame (`apps/SettingsApp.vue`) it is a size container, so this
 * follows the window's width, not the screen's. The width logic is `useSidebarFrame`. The
 * `sidebar` slot gets `run`, to wrap what a pick in the overlay does.
 */
import { headerControl } from '~/lib/wm/headerDom'

const { t } = useI18n()
const sidebarStore = useChatSidebarStore()

const frame = useTemplateRef<HTMLElement>('frame')
const panel = useTemplateRef<{ $el: HTMLElement }>('panel')

const { wideHidden, menuOpen, wide, visible, overlaying, toggle, close } =
  useSidebarFrame(frame, () => panel.value?.$el)

// FR-008: the hidden state outlives the tab; the overlay does not (data-model.md).
wideHidden.value = sidebarStore.hidden
watch(wideHidden, (hidden) => (sidebarStore.hidden = hidden))

const toggleLabel = computed(() =>
  visible.value ? t('chat.sidebar.hide') : t('chat.sidebar.show'),
)

const state = computed(() => {
  if (wide.value) return visible.value ? 'expanded' : 'hidden'
  return menuOpen.value ? 'overlay' : 'hidden'
})

// The button is in the window's header, like the other apps' sidebar buttons.
useWmHeader(
  () => ({
    start: [
      {
        type: 'toggle',
        id: 'sidebar',
        label: toggleLabel.value,
        icon: 'lucide:panel-left',
        pressed: visible.value,
        testId: 'chat-sidebar-toggle',
        collapse: { form: 'keep' },
      },
    ],
  }),
  (event) => {
    if (event.id === 'sidebar') toggle()
  },
)

/** For what is picked in the sidebar: the overlay closes, back to the content; a wide window keeps
 * its sidebar. */
function run(action: () => void) {
  close()
  action()
}

function onEscape() {
  if (!overlaying.value) return
  close()
  headerControl(frame.value, 'chat-sidebar-toggle')?.focus()
}
</script>

<template>
  <main ref="frame" class="@container relative flex h-full min-h-0 bg-muted/20">
    <WmSidebar
      id="chat-sidebar"
      ref="panel"
      :open="menuOpen"
      :wide-hidden="wideHidden"
      :aria-label="t('chat.sidebar.title')"
      :data-state="state"
      @escape="onEscape"
    >
      <slot name="sidebar" :run="run" />
    </WmSidebar>
    <div
      class="flex min-h-0 min-w-0 flex-1 flex-col"
      :inert="overlaying || undefined"
    >
      <slot />
    </div>
    <slot name="dialogs" />
  </main>
</template>
