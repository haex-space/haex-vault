<script setup lang="ts">
/**
 * The chat frame (spec 076): the thread sidebar beside the content, with no toolbar row of its own.
 * Like every app sidebar it can be hidden completely (`WmSidebar`): the button is in the window's
 * header. Wide, it sits beside the content; narrow, it opens over it, and the button and Escape
 * close it again. Like the settings frame (`apps/SettingsApp.vue`) it is a size container, so this
 * follows the window's width, not the screen's. The width logic is `useSidebarFrame`. The
 * `sidebar` slot gets `run`, to wrap what a pick in the overlay does. The sidebar button and the
 * button for a new conversation are in the window's header, like the other apps' buttons.
 */
import { headerControl } from '~/lib/wm/headerDom'
import { sidebarToggle } from '~/lib/wm/headerParts'

const emit = defineEmits<{ newConversation: [] }>()

const { t } = useI18n()
const sidebarStore = useChatSidebarStore()

const frame = useTemplateRef<HTMLElement>('frame')
const panel = useTemplateRef<{ $el: HTMLElement }>('panel')

const { wideHidden, menuOpen, wide, visible, overlaying, toggle, close } =
  useSidebarFrame(frame, () => panel.value?.$el)

// FR-008: the hidden state outlives the tab; the overlay does not (data-model.md).
wideHidden.value = sidebarStore.hidden
watch(wideHidden, (hidden) => (sidebarStore.hidden = hidden))

const state = computed(() => {
  if (wide.value) return visible.value ? 'expanded' : 'hidden'
  return menuOpen.value ? 'overlay' : 'hidden'
})

useWmHeader(
  () => ({
    start: [
      sidebarToggle({
        visible: visible.value,
        hideLabel: t('chat.sidebar.hide'),
        showLabel: t('chat.sidebar.show'),
        testId: 'chat-sidebar-toggle',
      }),
    ],
    end: [
      {
        type: 'button',
        id: 'new-conversation',
        label: t('chat.list.newConversation'),
        icon: 'lucide:square-pen',
        testId: 'chat-new-conversation',
      },
    ],
  }),
  (event) => {
    if (event.id === 'sidebar') toggle()
    else if (event.id === 'new-conversation') emit('newConversation')
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
