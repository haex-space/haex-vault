<script setup lang="ts">
/**
 * A secret that stays hidden until the user asks (spec 034, FR-005, US1 scenario 2): held with
 * the mouse, or tapped on a touch screen, or toggled with the keyboard. The value is fetched for
 * that moment, lives only in this component and ends when the pointer lets go (mouse), when the
 * entry or place is left, or when the entry changes. It is never written to the store, the title or
 * the history.
 */
import { toast } from 'vue-sonner'

const props = defineProps<{
  /** Fetches the value for the moment the user asks (one backend call; nothing is cached). */
  fetch: () => Promise<string>
  /** Changes when the secret it stands for changes (entry or field), which hides it again. */
  identity: string
  /** Whether the secret has a value at all; without one nothing can be revealed. */
  present: boolean
  label: string
  /** Part of the test ids. */
  kind: string
}>()

const { t } = useI18n()
const { errString } = useErrorString()

const value = ref<string | null>(null)
let token = 0

function hide() {
  token += 1
  value.value = null
}

async function showAsync() {
  const mine = ++token
  try {
    const revealed = await props.fetch()
    // A later hide or show outran this one.
    if (mine === token) value.value = revealed
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function toggle() {
  if (value.value === null) void showAsync()
  else hide()
}

type PointerCaptureTarget = {
  setPointerCapture: (pointerId: number) => void
  hasPointerCapture: (pointerId: number) => boolean
  releasePointerCapture: (pointerId: number) => void
}

function captureTarget(event: PointerEvent): PointerCaptureTarget | null {
  const target = event.currentTarget
  if (!target || typeof target !== 'object') return null
  if (
    !('setPointerCapture' in target) ||
    !('hasPointerCapture' in target) ||
    !('releasePointerCapture' in target)
  ) {
    return null
  }
  return target as PointerCaptureTarget
}

// The button captures a mouse hold, so a reveal that reflows the row does not count as leaving it.
// The hold ends on release, on cancel and whenever the capture is lost without a release (e.g. the
// window loses the pointer), so a value never stays shown without a held button.
function onPointerDown(event: PointerEvent) {
  if (event.pointerType === 'touch') return
  captureTarget(event)?.setPointerCapture(event.pointerId)
  void showAsync()
}

function onPointerEnd(event: PointerEvent) {
  if (event.pointerType === 'touch') return
  const target = captureTarget(event)
  if (
    event.type === 'pointerleave' &&
    target?.hasPointerCapture(event.pointerId)
  ) {
    return
  }
  if (target?.hasPointerCapture(event.pointerId)) {
    target.releasePointerCapture(event.pointerId)
  }
  hide()
}

function onClick(event: MouseEvent) {
  // Mouse and pen hold; a tap on a touch screen and the keyboard (detail 0) toggle.
  const pointerHold =
    event.detail > 0 && (event as PointerEvent).pointerType !== 'touch'
  if (!pointerHold) toggle()
}

watch(() => props.identity, hide)
onBeforeUnmount(hide)
</script>

<template>
  <span class="flex min-w-0 max-w-full items-center gap-2" data-no-swipe>
    <span
      class="min-w-0 flex-1 truncate font-mono text-sm"
      :class="value === null ? 'text-muted-foreground' : ''"
      :data-testid="`passwords-value-${kind}`"
    >
      <template v-if="!present">{{ t('passwords.noValue') }}</template>
      <template v-else-if="value === null">••••••••</template>
      <template v-else>{{ value }}</template>
    </span>
    <UiButton
      v-if="present"
      type="button"
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('passwords.reveal', { field: label })"
      :aria-pressed="value !== null"
      :tooltip="t('passwords.revealHint')"
      :data-testid="`passwords-reveal-${kind}`"
      @pointerdown="onPointerDown"
      @pointerup="onPointerEnd"
      @pointerleave="onPointerEnd"
      @pointercancel="onPointerEnd"
      @lostpointercapture="onPointerEnd"
      @click="onClick"
    >
      <Icon
        :name="value === null ? 'lucide:eye' : 'lucide:eye-off'"
        class="size-4"
      />
    </UiButton>
  </span>
</template>
