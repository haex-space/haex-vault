<script setup lang="ts">
/**
 * The header of a window (spec 090-app-header-bar): one row of 36 px that belongs to the app of the
 * active tab. It draws the controls the app described (`start`, `center`, `end`), the overflow
 * menu for those that do not fit, and the window buttons; without a description it shows the
 * title and the window buttons only (FR-027).
 *
 * How it collapses is `lib/wm/headerLayout.ts`'s (contracts/collapse.md); this measures and
 * applies. A hidden ghost row holds every control in its full and its icon form, so the widths are
 * real; the layout is derived (never stored) from them and the header's own width, so widening
 * gives every control back in its previous form (FR-017). A description is applied at most once
 * per animation frame (research R7), and the row stays `invisible` until the first layout so it
 * never flashes in an unfinished state. Controls are keyed by id: one that stays in the row keeps
 * its element and its focus while the others collapse (FR-016); one that moves into the menu
 * hands its focus to the menu button.
 *
 * A tab of a development extension carries the dev badge next to the window buttons (spec 017
 * FR-064), where the app's description cannot remove it.
 *
 * Dragging and double-clicking the empty parts are the window's (listeners on this component fall
 * through to the row); every control stops `pointerdown` itself.
 */
import {
  layoutHeader,
  minWidthOf,
  type HeaderArea,
  type LayoutControl,
} from '~/lib/wm/headerLayout'
import type { WmHeaderControl } from '~/types/bindings/WmHeaderControl'
import type { WmHeaderDescription } from '~/types/bindings/WmHeaderDescription'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

type Entry = { control: WmHeaderControl; area: HeaderArea; index: number }

const AREAS: readonly HeaderArea[] = ['start', 'center', 'end']
/** The gap between items, and (24 = 16 padding + one gap for the title's place) what the row keeps. */
const GAP = 8
const PADDING = 24

const props = defineProps<{
  title: string
  description: WmHeaderDescription | null
  maximized: boolean
  compact: boolean
  /** The tab whose app the description is from; search input is sent back to it. */
  owner?: string
  /** The window can be dragged by its header (not compact, not maximized). */
  movable: boolean
  /** The tab shows a development version of an extension: marked for good (spec 017 FR-064). */
  dev?: boolean
}>()

const emit = defineEmits<{
  /** `tab` is the tab a search input belongs to (it may no longer be the active one). */
  event: [event: WmHeaderEvent, tab?: string]
  minimize: []
  toggleMaximize: []
  close: []
}>()

const root = useTemplateRef<HTMLElement>('root')
const ghost = useTemplateRef<HTMLElement>('ghost')
const buttons = useTemplateRef<HTMLElement>('buttons')
const { width } = useElementSize(root, undefined, { box: 'border-box' })

const searches = useHeaderSearches((event, tab) => emit('event', event, tab))

/** Sends the search text of `tab` that is still waiting (the window manager calls it before the
 * tab's route changes). */
defineExpose({ flushSearch: (tab: string) => searches.flush(tab) })

/** What is drawn: the description most recently applied (at most one per animation frame). */
const shown = shallowRef<WmHeaderDescription | null>(props.description)
const entries = computed<Entry[]>(() =>
  AREAS.flatMap((area) =>
    (shown.value?.[area] ?? []).map((control, index) => ({
      control,
      area,
      index,
    })),
  ),
)

const widths = ref(new Map<string, { full: number; icon: number | null }>())
const fixed = ref({ windowButtons: 0, menuButton: 36 })
const laidOut = ref(false)

function describe(next: WmHeaderDescription | null, owner?: string) {
  shown.value = next
  searches.describe(
    AREAS.flatMap((area) => next?.[area] ?? []),
    owner,
  )
}
describe(props.description, props.owner)

let frame = 0
watch(
  () => [props.description, props.owner] as const,
  ([next, owner]) => {
    if (frame) cancelAnimationFrame(frame)
    frame = requestAnimationFrame(() => {
      frame = 0
      describe(next, owner)
    })
  },
)
onScopeDispose(() => {
  if (frame) cancelAnimationFrame(frame)
})

const hasIconForm = (control: WmHeaderControl) =>
  (control.type === 'button' ||
    control.type === 'toggle' ||
    control.type === 'menu') &&
  !!control.icon

/** Reads the widths the controls really have from the ghost row. */
function measure() {
  const g = ghost.value
  if (!g) return
  const widthOf = (id: string, form: string) =>
    g
      .querySelector<HTMLElement>(
        `[data-ghost="${CSS.escape(id)}"][data-form="${form}"]`,
      )
      ?.getBoundingClientRect().width ?? null
  widths.value = new Map(
    entries.value.map(({ control }) => [
      control.id,
      {
        full: widthOf(control.id, 'row') ?? 0,
        icon: widthOf(control.id, 'icon'),
      },
    ]),
  )
  fixed.value = {
    windowButtons: buttons.value?.offsetWidth ?? 0,
    menuButton:
      g.querySelector<HTMLElement>('[data-ghost-menu]')?.offsetWidth ?? 36,
  }
}

async function remeasure() {
  await nextTick()
  measure()
  laidOut.value = true
}

onMounted(remeasure)
watch(shown, remeasure)
watch(() => props.compact, remeasure)
// The dev badge widens the window buttons, which the collapse reads as a fixed width.
watch(() => props.dev, remeasure)

const layout = computed(() => {
  const controls: LayoutControl[] = entries.value.map(
    ({ control, area, index }) => ({
      id: control.id,
      area,
      index,
      order: control.collapse?.order ?? null,
      form: control.collapse?.form ?? 'overflow',
      group: control.collapse?.group ?? null,
      fullWidth: widths.value.get(control.id)?.full ?? 0,
      iconWidth: widths.value.get(control.id)?.icon ?? null,
      minWidth: minWidthOf(control.type),
    }),
  )
  return layoutHeader(
    controls,
    { ...fixed.value, gap: GAP, padding: PADDING },
    width.value,
  )
})

const stateOf = (control: WmHeaderControl) =>
  layout.value.states.get(control.id) ?? 'row'
const rowEntries = (area: HeaderArea) =>
  entries.value.filter(
    (e) => e.area === area && stateOf(e.control) !== 'overflow',
  )
const overflowControls = computed(() =>
  entries.value
    .filter((e) => stateOf(e.control) === 'overflow')
    .map((e) => e.control),
)
const formOf = (control: WmHeaderControl) =>
  stateOf(control) === 'icon' ? 'icon' : 'row'
const widthOf = (control: WmHeaderControl) => {
  const state = stateOf(control)
  return typeof state === 'object' ? state.shrunk : undefined
}
const searchOf = (control: WmHeaderControl) =>
  control.type === 'search'
    ? searches.fieldFor(control.id, control.value)
    : undefined

// A focused control that moves into the menu hands its focus to the menu button (FR-016).
watch(
  layout,
  async (next) => {
    const active = document.activeElement
    const id = active
      ?.closest('[data-header-control]')
      ?.getAttribute('data-header-control')
    if (!id || next.states.get(id) !== 'overflow') return
    await nextTick()
    root.value
      ?.querySelector<HTMLElement>('[data-testid="wm-header-overflow"]')
      ?.focus()
  },
  { flush: 'pre' },
)
</script>

<template>
  <div
    ref="root"
    role="toolbar"
    :aria-label="title"
    :title="title"
    class="relative flex h-9 shrink-0 items-center gap-2 border-b border-border bg-muted/40 px-2"
    :class="{ invisible: !laidOut }"
    :style="movable ? { touchAction: 'none' } : undefined"
    data-testid="wm-header"
  >
    <div
      v-if="rowEntries('start').length"
      class="flex shrink-0 items-center gap-2"
      data-area="start"
    >
      <div
        v-for="e in rowEntries('start')"
        :key="e.control.id"
        class="contents"
        :data-header-control="e.control.id"
      >
        <WmHeaderControl
          :control="e.control"
          :form="formOf(e.control)"
          :width="widthOf(e.control)"
          :icons="shown?.icons"
          :search="searchOf(e.control)"
          @event="emit('event', $event)"
        />
      </div>
    </div>

    <div
      class="flex min-w-0 flex-1 items-center justify-center gap-2"
      data-area="center"
    >
      <template v-if="rowEntries('center').length">
        <div
          v-for="e in rowEntries('center')"
          :key="e.control.id"
          class="contents"
          :data-header-control="e.control.id"
        >
          <WmHeaderControl
            :control="e.control"
            :form="formOf(e.control)"
            :width="widthOf(e.control)"
            :icons="shown?.icons"
            :search="searchOf(e.control)"
            @event="emit('event', $event)"
          />
        </div>
      </template>
      <span
        v-else
        class="min-w-0 truncate text-sm font-medium"
        data-testid="wm-header-title"
      >
        {{ title }}
      </span>
    </div>

    <div
      v-if="rowEntries('end').length || layout.menuVisible"
      class="flex shrink-0 items-center gap-2"
      data-area="end"
    >
      <div
        v-for="e in rowEntries('end')"
        :key="e.control.id"
        class="contents"
        :data-header-control="e.control.id"
      >
        <WmHeaderControl
          :control="e.control"
          :form="formOf(e.control)"
          :width="widthOf(e.control)"
          :icons="shown?.icons"
          :search="searchOf(e.control)"
          @event="emit('event', $event)"
        />
      </div>
      <WmHeaderOverflow
        v-if="layout.menuVisible"
        :controls="overflowControls"
        :icons="shown?.icons"
        :search-for="searchOf"
        @event="emit('event', $event)"
      />
    </div>

    <div ref="buttons" class="flex shrink-0 items-center gap-2">
      <WmDevBadge v-if="dev" />
      <WmWindowControls
        :maximized="maximized"
        :compact="compact"
        @minimize="emit('minimize')"
        @toggle-maximize="emit('toggleMaximize')"
        @close="emit('close')"
      />
    </div>

    <!-- Measures every control in its full and its icon form; never seen, never reached. -->
    <div
      ref="ghost"
      class="pointer-events-none invisible absolute left-0 top-0 flex flex-col items-start"
      aria-hidden="true"
      inert
    >
      <template v-for="e in entries" :key="e.control.id">
        <span class="inline-flex" :data-ghost="e.control.id" data-form="row">
          <WmHeaderControl
            :control="e.control"
            form="row"
            ghost
            :icons="shown?.icons"
            :search="searchOf(e.control)"
          />
        </span>
        <span
          v-if="hasIconForm(e.control)"
          class="inline-flex"
          :data-ghost="e.control.id"
          data-form="icon"
        >
          <WmHeaderControl
            :control="e.control"
            form="icon"
            ghost
            :icons="shown?.icons"
            :search="searchOf(e.control)"
          />
        </span>
      </template>
      <span class="inline-flex" data-ghost-menu>
        <UiButton variant="ghost" size="icon" tabindex="-1">
          <Icon name="lucide:ellipsis" class="size-4" />
        </UiButton>
      </span>
    </div>
  </div>
</template>
