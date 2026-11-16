<script setup lang="ts">
/**
 * The field of a header search (spec 090-app-header-bar, FR-008, FR-012): the search is only a
 * button in the header, and pressing it opens its input here, directly below the header in the
 * middle of the window. Like the overflow sidebar it belongs to the window (`Window.vue` provides
 * the element it is drawn in), so it lies over the content, not over the screen.
 *
 * Which search is open is `WM_SEARCH_OPEN` (a control id); the button, a key of the app and the
 * sidebar entry all set it. The field closes on Enter (the text stays, the button carries the dot),
 * on the second Escape (the first empties it) and on a click beside it, and with the control when
 * the app's next description no longer has it. Opening moves the focus into the field, closing
 * gives it back to the button when that is in the row, and tells the field it lost the focus so a
 * waiting text is sent and a newer description of the app can be applied (`headerSearch`).
 */
import { WM_OVERLAY, WM_SEARCH_OPEN } from '~/lib/wm/overlay'
import { DROP_FROM_TOP } from '~/lib/wm/slide'
import type { HeaderSearchField } from '~/composables/useHeaderSearches'
import type { WmHeaderControl } from '~/types/bindings/WmHeaderControl'

const props = defineProps<{
  controls: readonly WmHeaderControl[]
  icons?: Record<string, string>
  searchFor: (control: WmHeaderControl) => HeaderSearchField | undefined
}>()

const overlay = inject(WM_OVERLAY, null)
const openId = inject(WM_SEARCH_OPEN, ref<string | null>(null))
const panel = useTemplateRef<HTMLElement>('panel')

const control = computed(() =>
  props.controls.find((c) => c.type === 'search' && c.id === openId.value),
)
const field = computed(() =>
  control.value ? props.searchFor(control.value) : undefined,
)
const open = computed(() => !!control.value && !!field.value)

/** The control that is no longer there (another description, another tab) takes its open state along. */
watch(
  () => [openId.value, open.value] as const,
  ([id, isOpen]) => {
    if (id && !isOpen) openId.value = null
  },
  { flush: 'post' },
)

/** What was open last: closing needs the field and the button even when the description is gone. */
let last: { id: string; field: HeaderSearchField } | null = null

/**
 * Closing gives the focus back to the header, but only when it is nowhere: on the page itself or
 * still in the field that is leaving. A press on another element has put it there, and it stays.
 * The target is the search's button in the row, or the overflow toggle while the button is collapsed
 * into the sidebar (which closed when the search opened).
 */
function returnFocus(id: string) {
  const active = document.activeElement
  if (
    active &&
    active !== document.body &&
    !active.closest('[data-testid="wm-header-search-popover"]')
  )
    return
  const header = overlay?.value
    ?.closest('[data-wm-window-id]')
    ?.querySelector('[data-testid="wm-header"]')
  const target =
    header?.querySelector<HTMLElement>(
      `[data-header-control="${CSS.escape(id)}"] button`,
    ) ??
    header?.querySelector<HTMLElement>('[data-testid="wm-header-overflow"]')
  target?.focus()
}

// Watches the field itself, not its id: another tab's description with the same id has another field.
watch(
  () => (open.value ? (field.value ?? null) : null),
  async (current) => {
    const closed = last
    last = null
    if (closed) {
      closed.field.blur()
      if (!current) returnFocus(closed.id)
    }
    if (!current || !control.value) return
    last = { id: control.value.id, field: current }
    await nextTick()
    panel.value?.querySelector<HTMLElement>('input')?.focus()
  },
  { flush: 'post' },
)

function close() {
  openId.value = null
}

// A press anywhere else closes the field; the button toggles it itself.
onClickOutside(panel, close, {
  ignore: () => [`[data-header-control="${CSS.escape(openId.value ?? '')}"]`],
})
</script>

<template>
  <Teleport :to="overlay" :disabled="!overlay">
    <div
      class="pointer-events-none absolute inset-x-0 top-0 flex justify-center px-3 pt-2"
    >
      <Transition v-bind="DROP_FROM_TOP">
        <div
          v-if="open && control"
          ref="panel"
          role="dialog"
          :aria-label="control.label"
          class="pointer-events-auto w-full max-w-md rounded-lg border border-border bg-popover p-2 text-popover-foreground shadow-lg"
          data-testid="wm-header-search-popover"
        >
          <WmHeaderControl
            form="field"
            :control="control"
            :icons="icons"
            :search="field"
          />
        </div>
      </Transition>
    </div>
  </Teleport>
</template>
