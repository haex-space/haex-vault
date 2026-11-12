<script setup lang="ts">
/**
 * The codes of a pass in view mode (spec 092, FR-008, FR-009): each code drawn by the backend (no
 * network), dark on white in every theme, with its format and label, a button to show it large and
 * one to copy its value as text. A code that arrived flagged shows as text with a note
 * (FR-003). The codes load for this one pass when the view opens and again when the pass changes;
 * a value lives only in this component.
 */
import { toast } from 'vue-sonner'
import type { CodeView } from '@bindings/CodeView'

const props = defineProps<{
  itemId: string
  /** The update token of the loaded pass: a new one reloads the codes. */
  updatedAt: string | null
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { codesAsync } = usePasses()
const { copyText } = usePasswordsCopy()

const codes = ref<CodeView[]>([])
const loading = ref(true)
const large = ref<CodeView | null>(null)

function sourceOf(code: CodeView): string | null {
  return code.drawing
    ? `data:image/svg+xml;utf8,${encodeURIComponent(code.drawing.svg)}`
    : null
}

/** Only the newest request may fill the list: an older answer that arrives late is dropped. */
let generation = 0

async function loadAsync() {
  const mine = ++generation
  try {
    // The backend draws each code from the value it returns beside it, in one read, so a row
    // never shows a drawing of a value that another window has changed meanwhile.
    const loaded = await codesAsync(props.itemId)
    if (mine === generation) codes.value = loaded
  } catch (cause) {
    if (mine === generation) toast.error(errString(cause))
  } finally {
    if (mine === generation) loading.value = false
  }
}

onMounted(loadAsync)
watch(() => props.updatedAt, loadAsync)

function formatName(format: string): string {
  const known = ['qr', 'code128', 'ean13', 'pdf417', 'aztec'].includes(format)
  return known ? t(`passwords.pass.codeFormats.${format}`) : format
}
</script>

<template>
  <SettingsGroup
    v-if="loading || codes.length"
    :label="t('passwords.pass.codes')"
  >
    <li
      v-if="loading"
      class="flex justify-center px-4 py-6 text-muted-foreground"
      role="status"
    >
      <Icon name="lucide:loader-circle" class="size-5 animate-spin" />
    </li>
    <li
      v-for="(code, index) in codes"
      :key="code.id"
      class="flex flex-col gap-3 px-4 py-3"
      :data-testid="`passwords-pass-code-${index}`"
    >
      <div class="flex items-center gap-2">
        <div class="min-w-0 flex-1">
          <p class="truncate font-medium">
            {{ code.label || formatName(code.format) }}
          </p>
          <p v-if="code.label" class="truncate text-sm text-muted-foreground">
            {{ formatName(code.format) }}
          </p>
        </div>
        <UiButton
          v-if="code.drawing"
          variant="outline"
          size="sm"
          :data-testid="`passwords-pass-show-large-${index}`"
          @click="large = code"
        >
          <Icon name="lucide:maximize-2" class="size-4" />
          {{ t('passwords.pass.showLarge') }}
        </UiButton>
        <PasswordsCopyButton
          :label="t('passwords.pass.codeValue')"
          :data-testid="`passwords-pass-copy-code-${index}`"
          @copy="copyText(code.value, t('passwords.pass.codeValue'))"
        />
      </div>
      <!-- Dark on white in every theme: the ground is white whatever the theme is (FR-009). -->
      <div
        v-if="sourceOf(code)"
        class="flex justify-center rounded-lg p-2"
        style="background-color: #ffffff"
      >
        <img
          :src="sourceOf(code) ?? undefined"
          alt=""
          class="max-h-48 w-full max-w-[22rem] object-contain"
          style="image-rendering: pixelated"
          data-no-swipe
          :data-testid="`passwords-pass-code-image-${index}`"
        />
      </div>
      <div v-else class="flex flex-col gap-1" data-no-swipe>
        <p
          class="font-mono text-sm break-all select-text"
          :data-testid="`passwords-pass-code-text-${index}`"
        >
          {{ code.value }}
        </p>
        <p class="text-sm text-muted-foreground">
          {{
            code.flag === 'unknown_format'
              ? t('passwords.pass.flag.unknownFormat')
              : t('passwords.pass.flag.invalidValue')
          }}
        </p>
      </div>
    </li>
  </SettingsGroup>

  <PasswordsPassCodeLarge v-if="large" :code="large" @close="large = null" />
</template>
