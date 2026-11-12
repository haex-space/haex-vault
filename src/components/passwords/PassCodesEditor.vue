<script setup lang="ts">
/**
 * The codes of a pass in the editor (spec 092, FR-003, FR-012): add, change, reorder, remove, copy
 * the value as text. While the user types, the value is checked against its format by the backend
 * (the one rule, research R3) and a reason shows at the field; a code that arrived flagged shows the
 * same reason and stays repairable. A code keeps its id while it is edited, so two devices that
 * edit different codes merge row by row.
 */
import { CODE_FORMATS, type CodeDraft } from '~/lib/passwords/passDraft'

const props = defineProps<{
  /** The reason the backend refused a code with, by index in the list (`code_invalid`). */
  refused?: Record<number, string>
}>()
const codes = defineModel<CodeDraft[]>({ required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { checkCodeAsync } = usePasses()
const { copyText } = usePasswordsCopy()

const FORMAT_OPTIONS = CODE_FORMATS.map((format) => ({
  value: format,
  label: t(`passwords.pass.codeFormats.${format}`),
}))

/** The reason a typed value does not fit, by the index in the list. */
const reasons = ref<Record<number, string>>({})
const timers = new Map<number, ReturnType<typeof setTimeout>>()

/** Sets the reason of one code, or clears it with `null`. */
function setReason(index: number, reason: string | null) {
  const rest = Object.entries(reasons.value).filter(
    ([key]) => Number(key) !== index,
  )
  reasons.value = Object.fromEntries(
    reason === null ? rest : [...rest, [String(index), reason]],
  )
}

function schedule(index: number) {
  const code = codes.value[index]
  clearTimeout(timers.get(index))
  if (!code) {
    setReason(index, null)
    return
  }
  if (code.value.trim() === '') {
    // A new placeholder is left out of the save; a stored code that was emptied is refused, so it
    // says so here instead of being lost.
    setReason(index, code.id === null ? null : 'empty')
    return
  }
  timers.set(
    index,
    setTimeout(async () => {
      const { format, value } = code
      try {
        const result = await checkCodeAsync(format, value)
        // A later edit has replaced this answer.
        const now = codes.value[index]
        if (!now || now.format !== format || now.value !== value) return
        setReason(index, result.ok ? null : (result.reason ?? 'cannot_encode'))
      } catch {
        // Only the answer for the value that is there now may clear its reason.
        const now = codes.value[index]
        if (now && now.format === format && now.value === value)
          setReason(index, null)
      }
    }, 300),
  )
}

watch(
  () => codes.value.map((code) => `${code.format}\u0000${code.value}`),
  () => codes.value.forEach((_, index) => schedule(index)),
  { immediate: true },
)
onBeforeUnmount(() => timers.forEach((timer) => clearTimeout(timer)))

function reasonOf(index: number): string | null {
  const reason = props.refused?.[index] ?? reasons.value[index]
  return reason ? t(`passwords.pass.codeErrors.${reason}`) : null
}

function patch(index: number, change: Partial<CodeDraft>) {
  codes.value = codes.value.map((code, i) =>
    i === index ? { ...code, ...change } : code,
  )
}

function add() {
  codes.value = [
    ...codes.value,
    { id: null, format: 'qr', value: '', label: '' },
  ]
}

function remove(index: number) {
  codes.value = codes.value.filter((_, i) => i !== index)
}

function move(index: number, by: -1 | 1) {
  const target = index + by
  if (target < 0 || target >= codes.value.length) return
  const next = [...codes.value]
  const [moved] = next.splice(index, 1)
  if (moved) next.splice(target, 0, moved)
  codes.value = next
}
</script>

<template>
  <div class="flex flex-col gap-3" data-testid="passwords-pass-codes-editor">
    <div
      v-for="(code, index) in codes"
      :key="code.id ?? `new-${index}`"
      class="flex flex-col gap-2 rounded-xl bg-background/60 p-3"
      :data-testid="`passwords-pass-code-editor-${index}`"
    >
      <div class="flex flex-wrap items-center gap-2">
        <div class="min-w-36 flex-1">
          <UiSelect
            :model-value="code.format"
            :options="FORMAT_OPTIONS"
            :label="t('passwords.pass.codeFormat')"
            label-bg="var(--muted)"
            :data-testid="`passwords-pass-code-format-${index}`"
            @update:model-value="patch(index, { format: $event ?? 'qr' })"
          />
        </div>
        <div class="flex shrink-0 items-center">
          <UiButton
            type="button"
            variant="ghost"
            size="icon"
            :disabled="index === 0"
            :aria-label="t('passwords.pass.moveUp')"
            @click="move(index, -1)"
          >
            <Icon name="lucide:arrow-up" class="size-4" />
          </UiButton>
          <UiButton
            type="button"
            variant="ghost"
            size="icon"
            :disabled="index === codes.length - 1"
            :aria-label="t('passwords.pass.moveDown')"
            @click="move(index, 1)"
          >
            <Icon name="lucide:arrow-down" class="size-4" />
          </UiButton>
          <UiButton
            type="button"
            variant="ghost"
            size="icon"
            :aria-label="t('passwords.pass.removeCode')"
            :data-testid="`passwords-pass-code-remove-${index}`"
            @click="remove(index)"
          >
            <Icon name="lucide:trash-2" class="size-4" />
          </UiButton>
        </div>
      </div>
      <UiInput
        :model-value="code.value"
        :label="t('passwords.pass.codeValue')"
        :labels="fieldLabels.input.value"
        label-bg="var(--muted)"
        autocomplete="off"
        :data-testid="`passwords-pass-code-value-${index}`"
        @update:model-value="patch(index, { value: String($event ?? '') })"
      >
        <template v-if="code.value" #append>
          <PasswordsCopyButton
            :label="t('passwords.pass.codeValue')"
            @copy="copyText(code.value, t('passwords.pass.codeValue'))"
          />
        </template>
      </UiInput>
      <p
        v-if="reasonOf(index)"
        class="text-sm text-destructive"
        role="alert"
        :data-testid="`passwords-pass-code-error-${index}`"
      >
        {{ reasonOf(index) }}
      </p>
      <UiInput
        :model-value="code.label"
        :label="t('passwords.pass.codeLabel')"
        :labels="fieldLabels.input.value"
        label-bg="var(--muted)"
        :placeholder="t('passwords.pass.codeLabelPlaceholder')"
        :data-testid="`passwords-pass-code-label-${index}`"
        @update:model-value="patch(index, { label: String($event ?? '') })"
      />
    </div>
    <div>
      <UiButton
        type="button"
        variant="outline"
        size="sm"
        data-testid="passwords-pass-code-add"
        @click="add"
      >
        <Icon name="lucide:plus" class="size-4" />
        {{ t('passwords.pass.addCode') }}
      </UiButton>
    </div>
  </div>
</template>
