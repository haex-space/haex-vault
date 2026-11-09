<script setup lang="ts">
/**
 * The password generator (spec 034, US3, FR-013, FR-014): length, character classes, excluded
 * characters or a pattern, a live result, and saved presets with exactly one default that is
 * preselected. A choice that allows no output shows what to change under the field it concerns. Used in a
 * drawer of the editor (`embedded`, with "Übernehmen") and as the place `/generator`. The password
 * comes from the crate haex-passgen in Rust (the one generator, shared with haex-pass-browser) and
 * is kept only in this component and, on "Übernehmen", in the draft of the editor.
 */
import { toast } from 'vue-sonner'
import type { Preset } from '@bindings/Preset'
import type { Generated } from '@bindings/Generated'
import type { GeneratorSettings } from '@bindings/GeneratorSettings'

defineProps<{
  /** Shown in a drawer of the editor: offers to use the result. */
  embedded?: boolean
}>()

const emit = defineEmits<{
  use: [password: string]
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { errString } = useErrorString()
const { generateAsync, presetListAsync, presetSaveAsync, presetDeleteAsync } =
  usePasswords()
// Through Rust like every other copy, so the clipboard is cleared after the vault's delay.
const { copyText } = usePasswordsCopy()

/** The longest password outside pattern mode (haex-passgen `MAX_LENGTH`; longer ones are refused
 * with `length_range`). */
const MAX_LENGTH = 256

/** Without a preset: length 20 and all four classes. */
const DEFAULTS: GeneratorSettings = {
  length: 20,
  uppercase: true,
  lowercase: true,
  numbers: true,
  symbols: true,
  excludeChars: '',
  usePattern: false,
  pattern: '',
}

const config = reactive<GeneratorSettings>({ ...DEFAULTS })
const presets = ref<Preset[]>([])
const presetId = ref('')
const presetName = ref('')
const output = ref<Generated>({ value: '' })

/** The select cannot hold an empty value, so "no preset" gets a placeholder value. */
const NO_PRESET = '__none__'
const presetOptions = computed(() => [
  { value: NO_PRESET, label: t('passwords.generator.noPreset') },
  ...presets.value.map((preset) => ({
    value: preset.id,
    label: `${preset.name}${preset.isDefault ? ' ★' : ''}`,
  })),
])
const presetSelection = computed({
  get: () => presetId.value || NO_PRESET,
  set: (value: string | null | undefined) =>
    apply(presets.value.find((preset) => preset.id === value)),
})
/** The editor shows the generator in a dialog (popover surface); the page has none. */

/** Counts the requests, so an answer overtaken by a later change is dropped. */
let generation = 0

async function regenerateAsync() {
  const mine = ++generation
  try {
    const result = await generateAsync({ ...config })
    if (mine === generation) output.value = result
  } catch (cause) {
    if (mine === generation) toast.error(errString(cause))
  }
}

function regenerate() {
  void regenerateAsync()
}

const errorText = computed(() =>
  'error' in output.value
    ? t(`passwords.generator.errors.${output.value.error}`)
    : null,
)

/** The field whose value has to change, under which the hint shows (`output` for a code not known here). */
const FIELD_OF: Record<string, 'pattern' | 'length' | 'classes' | 'exclude'> = {
  pattern_empty: 'pattern',
  length_range: 'length',
  length_below_classes: 'length',
  no_class: 'classes',
  class_empty: 'exclude',
}
const errorField = computed(() => {
  if (!('error' in output.value)) return null
  const field = FIELD_OF[output.value.error]
  // A field that is not shown in the current mode cannot carry the hint.
  if (!field || config.usePattern !== (field === 'pattern')) return 'output'
  return field
})

function apply(preset: Preset | undefined) {
  Object.assign(
    config,
    preset
      ? {
          length: preset.length,
          uppercase: preset.uppercase,
          lowercase: preset.lowercase,
          numbers: preset.numbers,
          symbols: preset.symbols,
          excludeChars: preset.excludeChars,
          usePattern: preset.usePattern,
          pattern: preset.pattern,
        }
      : DEFAULTS,
  )
  presetId.value = preset?.id ?? ''
  presetName.value = preset?.name ?? ''
  regenerate()
}

async function loadAsync() {
  try {
    presets.value = await presetListAsync()
    apply(presets.value.find((preset) => preset.isDefault))
  } catch (cause) {
    toast.error(errString(cause))
    regenerate()
  }
}

async function saveAsync(makeDefault: boolean) {
  if (!presetName.value.trim()) return
  try {
    const saved = await presetSaveAsync({
      id: presetId.value,
      name: presetName.value,
      length: config.length,
      uppercase: config.uppercase,
      lowercase: config.lowercase,
      numbers: config.numbers,
      symbols: config.symbols,
      excludeChars: config.excludeChars,
      usePattern: config.usePattern,
      pattern: config.pattern,
      isDefault:
        makeDefault ||
        (presets.value.find((p) => p.id === presetId.value)?.isDefault ??
          false),
    })
    presets.value = await presetListAsync()
    presetId.value = saved.presetId
  } catch (cause) {
    const reason = (cause as { reason?: string }).reason
    toast.error(
      reason === 'name' || reason === 'length'
        ? t(`passwords.generator.presetErrors.${reason}`)
        : errString(cause),
    )
  }
}

async function removeAsync() {
  if (!presetId.value) return
  try {
    await presetDeleteAsync(presetId.value)
    presets.value = await presetListAsync()
    apply(presets.value.find((preset) => preset.isDefault))
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function copy() {
  if ('value' in output.value && output.value.value) {
    void copyText(output.value.value, t('passwords.fields.password'))
  }
}

watch(config, regenerate)
onMounted(loadAsync)
</script>

<template>
  <div class="flex flex-col gap-4" data-testid="passwords-generator">
    <div class="flex flex-col gap-2 rounded-xl bg-muted p-3">
      <output
        class="min-h-8 font-mono text-lg break-all"
        data-testid="passwords-generator-output"
        aria-live="polite"
      >
        <template v-if="'value' in output">{{ output.value }}</template>
        <span
          v-else-if="errorField === 'output'"
          class="text-sm text-destructive"
          role="alert"
          data-testid="passwords-generator-error"
          >{{ errorText }}</span
        >
      </output>
      <div class="flex flex-wrap gap-2">
        <UiButton
          variant="outline"
          size="sm"
          data-testid="passwords-generator-again"
          @click="regenerate"
        >
          <Icon name="lucide:refresh-cw" class="size-4" />
          {{ t('passwords.generator.again') }}
        </UiButton>
        <UiButton
          variant="outline"
          size="sm"
          :disabled="!('value' in output)"
          data-testid="passwords-generator-copy"
          @click="copy"
        >
          <Icon name="lucide:copy" class="size-4" />
          {{ t('passwords.generator.copy') }}
        </UiButton>
        <UiButton
          v-if="embedded"
          size="sm"
          :disabled="!('value' in output) || !output.value"
          data-testid="passwords-generator-use"
          @click="'value' in output && emit('use', output.value)"
        >
          {{ t('passwords.generator.use') }}
        </UiButton>
      </div>
    </div>

    <UiSelect
      id="pw-gen-preset"
      v-model="presetSelection"
      :options="presetOptions"
      :label="t('passwords.generator.preset')"
      data-testid="passwords-generator-preset"
    />

    <label class="flex items-center gap-3">
      <ShadcnCheckbox
        :model-value="config.usePattern"
        data-testid="passwords-generator-pattern-toggle"
        @update:model-value="config.usePattern = $event === true"
      />
      <span>{{ t('passwords.generator.usePattern') }}</span>
    </label>

    <template v-if="config.usePattern">
      <div class="flex flex-col gap-1.5">
        <UiInput
          id="pw-gen-pattern"
          v-model="config.pattern"
          :label="t('passwords.generator.pattern')"
          :labels="fieldLabels.input.value"
          spellcheck="false"
          data-testid="passwords-generator-pattern"
        />
        <p
          v-if="errorField === 'pattern'"
          class="text-xs text-destructive"
          role="alert"
          data-testid="passwords-generator-error"
        >
          {{ errorText }}
        </p>
        <p class="text-xs text-muted-foreground">
          {{ t('passwords.generator.patternHelp') }}
        </p>
      </div>
    </template>
    <template v-else>
      <div class="flex flex-col gap-1.5">
        <ShadcnLabel for="pw-gen-length">{{
          t('passwords.generator.length', { length: config.length })
        }}</ShadcnLabel>
        <input
          id="pw-gen-length"
          v-model.number="config.length"
          type="range"
          min="1"
          :max="MAX_LENGTH"
          class="accent-primary"
          data-testid="passwords-generator-length"
        />
        <p
          v-if="errorField === 'length'"
          class="text-xs text-destructive"
          role="alert"
          data-testid="passwords-generator-error"
        >
          {{ errorText }}
        </p>
      </div>
      <div class="flex flex-col gap-1.5">
        <div class="grid gap-2 @sm:grid-cols-2">
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="config.uppercase"
              @update:model-value="config.uppercase = $event === true"
            />
            <span>{{ t('passwords.generator.uppercase') }}</span>
          </label>
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="config.lowercase"
              @update:model-value="config.lowercase = $event === true"
            />
            <span>{{ t('passwords.generator.lowercase') }}</span>
          </label>
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="config.numbers"
              @update:model-value="config.numbers = $event === true"
            />
            <span>{{ t('passwords.generator.numbers') }}</span>
          </label>
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="config.symbols"
              @update:model-value="config.symbols = $event === true"
            />
            <span>{{ t('passwords.generator.symbols') }}</span>
          </label>
        </div>
        <p
          v-if="errorField === 'classes'"
          class="text-xs text-destructive"
          role="alert"
          data-testid="passwords-generator-error"
        >
          {{ errorText }}
        </p>
      </div>
      <div class="flex flex-col gap-1.5">
        <UiInput
          id="pw-gen-exclude"
          v-model="config.excludeChars"
          :label="t('passwords.generator.exclude')"
          :labels="fieldLabels.input.value"
          spellcheck="false"
          data-testid="passwords-generator-exclude"
        />
        <p
          v-if="errorField === 'exclude'"
          class="text-xs text-destructive"
          role="alert"
          data-testid="passwords-generator-error"
        >
          {{ errorText }}
        </p>
      </div>
    </template>

    <div class="flex flex-col gap-2 rounded-xl border border-border p-3">
      <UiInput
        id="pw-gen-name"
        v-model="presetName"
        :label="t('passwords.generator.presetName')"
        :labels="fieldLabels.input.value"
        data-testid="passwords-generator-name"
      />
      <div class="flex flex-wrap gap-2">
        <UiButton
          size="sm"
          variant="outline"
          :disabled="!presetName.trim()"
          data-testid="passwords-generator-save"
          @click="saveAsync(false)"
        >
          {{ t('passwords.generator.save') }}
        </UiButton>
        <UiButton
          size="sm"
          variant="outline"
          :disabled="!presetName.trim()"
          data-testid="passwords-generator-default"
          @click="saveAsync(true)"
        >
          {{ t('passwords.generator.makeDefault') }}
        </UiButton>
        <UiButton
          size="sm"
          variant="ghost"
          :disabled="!presetId"
          data-testid="passwords-generator-delete"
          @click="removeAsync"
        >
          {{ t('passwords.generator.delete') }}
        </UiButton>
      </div>
    </div>
  </div>
</template>
