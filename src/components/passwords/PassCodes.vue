<script setup lang="ts">
/**
 * The codes of a pass in view mode (spec 092, FR-008, FR-009): each code drawn by the backend (no
 * network), dark on white in every theme, with its format and label, a button to show it large and
 * one to copy its value as text. A code that arrived flagged shows as text with a note
 * (FR-003). The codes load for this one pass when the view opens and again when the pass changes;
 * a value lives only in this component.
 */
import { toast } from 'vue-sonner'
import type { CodeSvg } from '@bindings/CodeSvg'
import type { CodeView } from '@bindings/CodeView'

const props = defineProps<{
  itemId: string
  /** The update token of the loaded pass: a new one reloads the codes. */
  updatedAt: string | null
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { codesAsync, codeSvgAsync } = usePasses()
const { copyText } = usePasswordsCopy()

const codes = ref<CodeView[]>([])
const drawings = ref<Record<string, CodeSvg>>({})
const loading = ref(true)
const large = ref<CodeView | null>(null)

function sourceOf(drawing: CodeSvg | undefined): string | null {
  return drawing?.svg
    ? `data:image/svg+xml;utf8,${encodeURIComponent(drawing.svg)}`
    : null
}

async function loadAsync() {
  try {
    const loaded = await codesAsync(props.itemId)
    const next: Record<string, CodeSvg> = {}
    await Promise.all(
      loaded.map(async (code) => {
        next[code.id] = await codeSvgAsync(props.itemId, code.id)
      }),
    )
    codes.value = loaded
    drawings.value = next
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    loading.value = false
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
          v-if="drawings[code.id]?.svg"
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
        v-if="sourceOf(drawings[code.id])"
        class="flex justify-center rounded-lg bg-white p-2"
      >
        <img
          :src="sourceOf(drawings[code.id]) ?? undefined"
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

  <PasswordsPassCodeLarge
    v-if="large"
    :item-id="itemId"
    :code-id="large.id"
    :label="large.label"
    :format="large.format"
    @close="large = null"
  />
</template>
