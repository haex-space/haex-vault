<script setup lang="ts">
/**
 * The floating indicator of the voice assistant (spec 066 T047, FR-041): above windows and dock,
 * below dialogs. One look per phase (Lucide icons until the sprites, T118), with the phase as text
 * for screen readers and as tooltip. Dragging moves it; the position is the device preference
 * `assistant.bubble.position` (share 0–1 of the desktop). A click opens the compact transcript.
 * Until the wake word, it shows only with the preview setting on (T045) or while a conversation
 * runs.
 */
import {
  BUBBLE_DEFAULT,
  BUBBLE_POSITION_KEY,
  parseBubblePosition,
  PHASE_ICONS,
  shareOf,
  SPINNING_PHASES,
} from '~/lib/assistant/bubble'

const { t } = useI18n()
const assistant = useAssistantStore()
const { getPrefAsync, setPrefAsync } = usePreferences()
const { currentDeviceInfoAsync } = useDevice()

/** Diameter of the indicator in pixels. */
const SIZE = 48
/** Movement below this many pixels is a click, not a drag. */
const DRAG_THRESHOLD = 4

const area = useTemplateRef<HTMLElement>('area')
const panel = useTemplateRef<HTMLElement>('panel')
const { width, height } = useElementSize(area)
const position = ref(BUBBLE_DEFAULT)
const open = ref(false)

const visible = computed(() => assistant.preview || assistant.state.enabled)
const phase = computed(() => assistant.state.phase)
const label = computed(() => t(`assistant.phase.${phase.value}`))

/** The indicator's top left corner, kept inside the desktop. */
const place = computed(() => ({
  left: `${Math.min(Math.max(0, position.value.x * width.value - SIZE / 2), Math.max(0, width.value - SIZE))}px`,
  top: `${Math.min(Math.max(0, position.value.y * height.value - SIZE / 2), Math.max(0, height.value - SIZE))}px`,
}))
/** The panel opens towards the middle of the desktop. */
const panelSide = computed(() => ({
  above: position.value.y > 0.5,
  left: position.value.x > 0.5,
}))

async function deviceScopeAsync() {
  return {
    kind: 'device' as const,
    uuid: (await currentDeviceInfoAsync()).vaultDeviceUuid,
  }
}

let drag: { startX: number; startY: number; moved: boolean } | null = null

function onPointerDown(event: PointerEvent) {
  if (event.button !== 0) return
  drag = { startX: event.clientX, startY: event.clientY, moved: false }
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}

function onPointerMove(event: PointerEvent) {
  if (!drag || !area.value) return
  const distance = Math.hypot(
    event.clientX - drag.startX,
    event.clientY - drag.startY,
  )
  if (!drag.moved && distance < DRAG_THRESHOLD) return
  drag.moved = true
  const rect = area.value.getBoundingClientRect()
  position.value = shareOf(
    { x: event.clientX - rect.left, y: event.clientY - rect.top },
    rect.width,
    rect.height,
  )
}

async function onPointerUp() {
  const finished = drag
  drag = null
  if (!finished) return
  if (!finished.moved) {
    open.value = !open.value
    return
  }
  try {
    // action-exempt: where the indicator sits is this device's own layout choice; spec 066 gives agents no action for it.
    await setPrefAsync(
      await deviceScopeAsync(),
      BUBBLE_POSITION_KEY,
      JSON.stringify(position.value),
    )
  } catch (error) {
    console.error('[assistant] bubble position', error)
  }
}

async function loadPositionAsync() {
  try {
    position.value = parseBubblePosition(
      await getPrefAsync(await deviceScopeAsync(), BUBBLE_POSITION_KEY),
    )
  } catch {
    position.value = BUBBLE_DEFAULT
  }
}

onClickOutside(panel, () => (open.value = false), {
  ignore: ['[data-testid="assistant-bubble"]'],
})

onMounted(async () => {
  await assistant.start()
  await loadPositionAsync()
})
</script>

<template>
  <div
    ref="area"
    class="pointer-events-none absolute inset-0 z-30"
    data-testid="assistant-bubble-area"
  >
    <div v-if="visible" class="absolute" :style="place">
      <button
        type="button"
        class="pointer-events-auto relative flex size-12 touch-none items-center justify-center rounded-full border bg-background shadow-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="{
          'ring-2 ring-primary': phase === 'listening' || phase === 'followUp',
          'border-primary': phase === 'awaitingAnswer',
        }"
        :aria-label="label"
        :title="label"
        :aria-expanded="open"
        data-testid="assistant-bubble"
        :data-phase="phase"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @keydown.enter.prevent="open = !open"
        @keydown.space.prevent="open = !open"
      >
        <Icon
          :name="PHASE_ICONS[phase]"
          class="size-6"
          :class="{ 'animate-spin': SPINNING_PHASES.includes(phase) }"
          :aria-hidden="true"
        />
        <Icon
          v-if="assistant.state.muted"
          name="lucide:mic-off"
          class="absolute -right-1 -bottom-1 size-4 rounded-full bg-background text-destructive"
          :aria-hidden="true"
        />
      </button>
      <div
        v-if="open"
        ref="panel"
        class="pointer-events-auto absolute"
        :class="[
          panelSide.above ? 'bottom-14' : 'top-14',
          panelSide.left ? 'right-0' : 'left-0',
        ]"
      >
        <AssistantCompactTranscript @close="open = false" />
      </div>
    </div>
  </div>
</template>
