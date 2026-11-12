<script setup lang="ts">
/**
 * One code of a pass shown large (spec 092, FR-008, FR-009): the whole window on a white ground, the
 * code in its drawing with its label, the screen at full brightness and kept on while it is open,
 * and both given back when it closes. The drawing is dark on white in every theme (the ground is
 * white whatever the theme is), because a scanner needs the contrast. A code that could not be
 * drawn (it arrived flagged) shows as text.
 */
import { toast } from 'vue-sonner'
import type { CodeSvg } from '@bindings/CodeSvg'

const props = defineProps<{
  itemId: string
  codeId: string
  label: string | null
  format: string
}>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const { codeSvgAsync } = usePasses()
const { onAsync, offAsync } = useScreenBright()

const drawing = ref<CodeSvg | null>(null)
const failed = ref(false)

const source = computed(() =>
  drawing.value?.svg
    ? `data:image/svg+xml;utf8,${encodeURIComponent(drawing.value.svg)}`
    : null,
)

async function loadAsync() {
  try {
    drawing.value = await codeSvgAsync(props.itemId, props.codeId)
  } catch (cause) {
    failed.value = true
    toast.error(errString(cause))
  }
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') emit('close')
}

onMounted(async () => {
  window.addEventListener('keydown', onKey)
  await Promise.all([loadAsync(), onAsync()])
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  void offAsync()
})
</script>

<template>
  <Teleport to="body">
    <div
      class="fixed inset-0 z-[100] flex flex-col items-center justify-center gap-6 bg-white p-6 text-black"
      role="dialog"
      aria-modal="true"
      :aria-label="label ?? t('passwords.pass.codeFormats.' + format)"
      data-testid="passwords-pass-large"
      @click="emit('close')"
    >
      <button
        type="button"
        class="absolute top-4 right-4 flex size-11 items-center justify-center rounded-full bg-black/5 text-black hover:bg-black/10"
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
        v-else-if="drawing?.text"
        class="max-w-full text-center font-mono text-2xl break-all select-text"
        data-testid="passwords-pass-large-text"
      >
        {{ drawing.text }}
      </p>
      <Icon
        v-else-if="!failed"
        name="lucide:loader-circle"
        class="size-8 animate-spin"
      />
      <p v-if="label" class="text-center text-xl font-medium">{{ label }}</p>
    </div>
  </Teleport>
</template>
