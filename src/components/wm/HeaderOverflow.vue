<script setup lang="ts">
/**
 * The overflow sidebar of a window header (spec 090-app-header-bar, FR-014, research R10): the
 * `panel-right` toggle and, behind it, the window's right sidebar holding the controls that did not
 * fit, each in its menu form. It is the app's sidebar on the left (`Sidebar.vue`, `slide.ts`) from
 * the other side: below `@2xl` it lies over the content and slides in from the right, from `@2xl` it
 * is a column beside the content, and the toggle is pressed while it shows. Unlike the app's sidebar
 * it is there only while a control has collapsed, so only a window that is small enough has it, and
 * it starts closed. `Window.vue` provides the place beside the content it is drawn in.
 *
 * Over the content it closes after a button, toggle, choice or menu entry, on Escape and on the
 * toggle; as a column it stays until the toggle or Escape closes it. A search entry opens its field
 * under the header and closes the sidebar (`Window.vue`). Opening moves the focus into the sidebar,
 * closing gives it back to the toggle.
 */
import { hasActiveMenu } from '~/lib/wm/headerLayout'
import { WM_OVERFLOW_OPEN, WM_SIDEBAR_HOST } from '~/lib/wm/overlay'
import type { HeaderSearchField } from '~/composables/useHeaderSearches'
import type { WmHeaderControl } from '~/types/bindings/WmHeaderControl'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

const props = defineProps<{
  controls: readonly WmHeaderControl[]
  icons?: Record<string, string>
  searchFor: (control: WmHeaderControl) => HeaderSearchField | undefined
}>()

const emit = defineEmits<{ event: [event: WmHeaderEvent] }>()

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])'

const { t } = useI18n()
const host = inject(WM_SIDEBAR_HOST, null)
const open = inject(WM_OVERFLOW_OPEN, ref(false))
/** A control in the sidebar is an active menu: the toggle carries its dot (FR-008). */
const active = computed(() => hasActiveMenu(props.controls))
const anchor = useTemplateRef<HTMLElement>('anchor')
const panel = useTemplateRef<{ $el: HTMLElement }>('panel')

/** The sidebar lies over the content (read from its CSS, like `useSidebarFrame` does). */
function overlaying(): boolean {
  const el = panel.value?.$el
  return !!el && getComputedStyle(el).position === 'absolute'
}

function onEvent(event: WmHeaderEvent) {
  emit('event', event)
  if (event.kind !== 'input' && event.kind !== 'submit' && overlaying()) close()
}

async function toggle() {
  if (open.value) return close()
  open.value = true
  await nextTick()
  // The sidebar fades in by a `visibility` transition (`slide.ts`), which still reads `hidden` in
  // the frame it starts: an element that is hidden refuses the focus, and Escape, which only the
  // sidebar hears, would then never close it. One frame on, it is visible.
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  panel.value?.$el.querySelector<HTMLElement>(FOCUSABLE)?.focus()
}

function close() {
  if (!open.value) return
  open.value = false
  anchor.value?.querySelector<HTMLElement>('button')?.focus()
}

// The sidebar belongs to the collapsed controls: with none, there is nothing to keep open.
onBeforeUnmount(() => {
  open.value = false
})

function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape') return
  event.preventDefault()
  event.stopPropagation()
  close()
}
</script>

<template>
  <span ref="anchor" class="contents">
    <UiButton
      variant="ghost"
      size="icon"
      :class="[
        'relative shrink-0',
        open && 'bg-primary/10 text-primary hover:bg-primary/15',
      ]"
      :aria-label="
        active
          ? `${t('wm.header.overflow')} (${t('wm.header.active')})`
          : t('wm.header.overflow')
      "
      :aria-expanded="open"
      :tooltip="t('wm.header.overflow')"
      data-testid="wm-header-overflow"
      @pointerdown.stop
      @click="toggle"
    >
      <Icon
        name="lucide:panel-right"
        class="size-4 text-primary"
        :aria-hidden="true"
      />
      <WmHeaderDot v-if="active" corner />
    </UiButton>
  </span>
  <Teleport :to="host" :disabled="!host">
    <WmSidebar
      ref="panel"
      side="right"
      :open="open"
      :wide-hidden="!open"
      :aria-label="t('wm.header.overflow')"
      data-testid="wm-header-overflow-menu"
      @keydown="onKeydown"
    >
      <div class="flex h-full flex-col gap-1 overflow-y-auto p-2">
        <WmHeaderControl
          v-for="control in controls"
          :key="control.id"
          form="menu"
          :control="control"
          :icons="icons"
          :search="searchFor(control)"
          @event="onEvent"
        />
      </div>
    </WmSidebar>
  </Teleport>
</template>
