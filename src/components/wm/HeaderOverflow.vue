<script setup lang="ts">
/**
 * The overflow menu of a window header (spec 090-app-header-bar, FR-014, research R10): the `⋯`
 * button and a panel that slides in from the window's right edge, over the window's content below
 * the header, holding the controls that did not fit, each in its menu form. The panel belongs to
 * the window (`Window.vue` provides the element it is drawn in), not to the screen, and it is as
 * tall as the window, so it works on a phone, where a large popover is hard to hit. It closes after
 * a button, toggle, choice or menu entry, on Escape and on a click beside it; a search stays open,
 * so typing is not cut off. Opening moves the focus into the panel, closing gives it back to the
 * `⋯` button.
 */
import { WM_OVERFLOW_OPEN, WM_OVERLAY } from '~/lib/wm/overlay'
import type { HeaderSearchField } from '~/composables/useHeaderSearches'
import type { WmHeaderControl } from '~/types/bindings/WmHeaderControl'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

defineProps<{
  controls: readonly WmHeaderControl[]
  icons?: Record<string, string>
  searchFor: (control: WmHeaderControl) => HeaderSearchField | undefined
}>()

const emit = defineEmits<{ event: [event: WmHeaderEvent] }>()

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])'

const { t } = useI18n()
const overlay = inject(WM_OVERLAY, null)
const open = inject(WM_OVERFLOW_OPEN, ref(false))
const anchor = useTemplateRef<HTMLElement>('anchor')
const panel = useTemplateRef<HTMLElement>('panel')

function onEvent(event: WmHeaderEvent) {
  emit('event', event)
  if (event.kind !== 'input' && event.kind !== 'submit') close()
}

async function toggle() {
  if (open.value) return close()
  open.value = true
  await nextTick()
  panel.value?.querySelector<HTMLElement>(FOCUSABLE)?.focus()
}

function close() {
  if (!open.value) return
  open.value = false
  anchor.value?.querySelector<HTMLElement>('button')?.focus()
}

// A press anywhere else (the sidebar button, another control, the content) closes the panel, so it
// never stays open under whatever the user turned to; the `⋯` button toggles it itself.
onClickOutside(panel, close, { ignore: [anchor] })

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
      class="shrink-0"
      :aria-label="t('wm.header.overflow')"
      :aria-expanded="open"
      :tooltip="t('wm.header.overflow')"
      data-testid="wm-header-overflow"
      @pointerdown.stop
      @click="toggle"
    >
      <Icon
        name="lucide:ellipsis"
        class="size-4 text-primary"
        :aria-hidden="true"
      />
    </UiButton>
  </span>
  <Teleport :to="overlay" :disabled="!overlay">
    <Transition
      enter-active-class="transition-transform duration-200 ease-out motion-reduce:transition-none"
      leave-active-class="transition-transform duration-150 ease-in motion-reduce:transition-none"
      enter-from-class="translate-x-full"
      leave-to-class="translate-x-full"
    >
      <div
        v-if="open"
        class="pointer-events-auto absolute inset-0 overflow-hidden"
        @keydown="onKeydown"
      >
        <button
          type="button"
          class="absolute inset-0 bg-foreground/25"
          tabindex="-1"
          :aria-label="t('wm.header.closeOverflow')"
          @click="close"
        />
        <div
          ref="panel"
          role="dialog"
          :aria-label="t('wm.header.overflow')"
          class="absolute inset-y-0 right-0 flex w-full flex-col border-l border-border bg-background shadow-lg @2xl:w-80"
          data-testid="wm-header-overflow-menu"
        >
          <div
            class="flex shrink-0 items-center justify-between gap-2 border-b border-border py-1 pr-1 pl-3"
          >
            <span class="min-w-0 truncate text-sm font-medium">{{
              t('wm.header.overflow')
            }}</span>
            <UiButton
              variant="ghost"
              size="icon"
              :aria-label="t('wm.header.closeOverflow')"
              :tooltip="t('wm.header.closeOverflow')"
              data-testid="wm-header-overflow-close"
              @click="close"
            >
              <Icon name="lucide:x" class="size-4" :aria-hidden="true" />
            </UiButton>
          </div>
          <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto p-2">
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
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
