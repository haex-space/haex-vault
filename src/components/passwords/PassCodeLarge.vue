<script setup lang="ts">
/**
 * One code of a pass shown large (spec 092, FR-008, FR-009): the whole window on a white ground, the
 * code in its drawing with its label, the screen at full brightness and kept on while it is open,
 * and both given back when it closes. The drawing is dark on white in every theme (the ground is
 * white whatever the theme is), because a scanner needs the contrast. A code that could not be
 * drawn (it arrived flagged) shows as text.
 */
import type { CodeView } from '@bindings/CodeView'

const props = defineProps<{
  /** The code as the row shows it, with the drawing made from the same value. */
  code: CodeView
}>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const { onAsync, offAsync } = useScreenBright()

const closeButton = ref<HTMLButtonElement | null>(null)
let opener: Element | null = null

const source = computed(() =>
  props.code.drawing
    ? `data:image/svg+xml;utf8,${encodeURIComponent(props.code.drawing.svg)}`
    : null,
)

/** The dialog is modal: Escape closes it and Tab stays on its one control instead of reaching
 * what lies behind the white sheet. */
function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') emit('close')
  if (event.key === 'Tab') {
    event.preventDefault()
    closeButton.value?.focus()
  }
}

onMounted(async () => {
  opener = document.activeElement
  window.addEventListener('keydown', onKey)
  closeButton.value?.focus()
  await onAsync()
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  void offAsync()
  if (opener instanceof HTMLElement) opener.focus()
})
</script>

<template>
  <Teleport to="body">
    <div
      class="fixed inset-0 z-[100] flex flex-col items-center justify-center gap-6 p-6"
      style="background-color: #ffffff; color: #000000"
      role="dialog"
      aria-modal="true"
      :aria-label="code.label ?? t('passwords.pass.codeFormats.' + code.format)"
      data-testid="passwords-pass-large"
      @click="emit('close')"
    >
      <button
        ref="closeButton"
        type="button"
        class="absolute top-4 right-4 flex size-11 items-center justify-center rounded-full hover:opacity-70"
        style="background-color: rgb(0 0 0 / 6%); color: #000000"
        :aria-label="t('passwords.pass.closeLarge')"
        data-testid="passwords-pass-large-close"
        @click.stop="emit('close')"
      >
        <Icon name="lucide:x" class="size-6" />
      </button>
      <img
        v-if="source"
        :src="source"
        alt=""
        class="max-h-[70vh] w-full max-w-[min(92vw,34rem)] object-contain"
        style="image-rendering: pixelated"
        data-testid="passwords-pass-large-code"
      />
      <p
        v-else
        class="max-w-full text-center font-mono text-2xl break-all select-text"
        data-testid="passwords-pass-large-text"
      >
        {{ code.value }}
      </p>
      <p v-if="code.label" class="text-center text-xl font-medium">
        {{ code.label }}
      </p>
    </div>
  </Teleport>
</template>
