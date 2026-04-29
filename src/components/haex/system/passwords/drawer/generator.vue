<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
  >
    <template #body>
      <div class="space-y-4">
        <!-- Preset picker -->
        <div
          v-if="presets.length > 0"
          class="space-y-1.5"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('loadPreset') }}
          </p>
          <div class="flex items-center gap-2">
            <USelectMenu
              v-model="selectedPresetId"
              :items="presetItems"
              :placeholder="t('selectPreset')"
              value-key="value"
              class="flex-1"
              @update:model-value="onPresetSelected"
            />
            <UiButton
              v-if="selectedPresetId"
              :tooltip="t('deletePreset')"
              icon="i-lucide-trash-2"
              color="error"
              variant="ghost"
              type="button"
              @click="onDeletePreset"
            />
          </div>
        </div>

        <!-- Live preview -->
        <div class="space-y-1.5">
          <p class="text-xs font-medium text-muted">
            {{ t('preview') }}
          </p>
          <div
            class="flex items-center gap-1 px-3 py-2 rounded-md bg-elevated/40 border border-default"
          >
            <span
              class="flex-1 font-mono text-base break-all select-all min-h-6"
            >{{ preview || '&nbsp;' }}</span>
            <UiButton
              :tooltip="t('regenerate')"
              icon="i-lucide-refresh-cw"
              color="neutral"
              variant="ghost"
              type="button"
              @click="regenerate"
            />
            <UiButton
              :tooltip="copied ? t('copied') : t('copy')"
              :icon="copied ? 'i-lucide-check' : 'i-lucide-copy'"
              :color="copied ? 'success' : 'neutral'"
              variant="ghost"
              type="button"
              @click="copyPreview"
            />
          </div>
        </div>

        <!-- Length -->
        <div v-if="!config.usePattern">
          <div class="flex items-center justify-between mb-1.5">
            <p class="text-xs font-medium text-muted">
              {{ t('length') }}
            </p>
            <span class="text-sm font-mono tabular-nums">{{ config.length }}</span>
          </div>
          <USlider
            v-model="config.length"
            :min="4"
            :max="128"
            :step="1"
          />
        </div>

        <!-- Char-type toggles -->
        <div v-if="!config.usePattern">
          <p class="text-xs font-medium text-muted mb-1.5">
            {{ t('characterTypes') }}
          </p>
          <div class="flex flex-wrap gap-2">
            <UiButton
              :label="'A-Z'"
              :color="config.uppercase ? 'primary' : 'neutral'"
              :variant="config.uppercase ? 'solid' : 'outline'"
              type="button"
              @click="config.uppercase = !config.uppercase"
            />
            <UiButton
              :label="'a-z'"
              :color="config.lowercase ? 'primary' : 'neutral'"
              :variant="config.lowercase ? 'solid' : 'outline'"
              type="button"
              @click="config.lowercase = !config.lowercase"
            />
            <UiButton
              :label="'0-9'"
              :color="config.numbers ? 'primary' : 'neutral'"
              :variant="config.numbers ? 'solid' : 'outline'"
              type="button"
              @click="config.numbers = !config.numbers"
            />
            <UiButton
              :label="'!@#'"
              :color="config.symbols ? 'primary' : 'neutral'"
              :variant="config.symbols ? 'solid' : 'outline'"
              type="button"
              @click="config.symbols = !config.symbols"
            />
          </div>
        </div>

        <!-- Exclude chars -->
        <div v-if="!config.usePattern">
          <UiInput
            v-model="config.excludeChars"
            :label="t('excludeChars')"
            :placeholder="t('excludeCharsPlaceholder')"
          />
        </div>

        <USeparator />

        <!-- Pattern mode -->
        <div class="space-y-2">
          <div class="flex items-center gap-2">
            <UCheckbox
              v-model="config.usePattern"
              :label="t('usePattern')"
            />
            <UPopover :content="{ side: 'top', align: 'start' }">
              <UButton
                icon="i-lucide-info"
                :aria-label="t('patternHelpToggle')"
                color="neutral"
                variant="ghost"
                type="button"
              />
              <template #content>
                <div class="p-3 space-y-1.5 max-w-xs text-sm">
                  <p class="font-semibold">
                    {{ t('patternHelpTitle') }}
                  </p>
                  <ul class="space-y-1">
                    <li
                      v-for="key in patternHelpKeys"
                      :key="key"
                      class="flex gap-2"
                    >
                      <code
                        class="px-1.5 py-0.5 rounded bg-elevated font-mono text-xs shrink-0"
                      >{{ key }}</code>
                      <span>{{ t(`patternHelp.${key}`) }}</span>
                    </li>
                    <li class="text-muted text-xs pt-1">
                      {{ t('patternHelp.other') }}
                    </li>
                  </ul>
                </div>
              </template>
            </UPopover>
          </div>

          <UiInput
            v-if="config.usePattern"
            v-model="config.pattern"
            :label="t('pattern')"
            :placeholder="t('patternPlaceholder')"
          />
        </div>

        <USeparator />

        <!-- Save as preset -->
        <div class="space-y-2">
          <p class="text-xs font-medium text-muted">
            {{ t('saveAsPreset') }}
          </p>
          <UiInput
            v-model="presetName"
            :placeholder="t('presetNamePlaceholder')"
          />
          <div class="flex items-center justify-between gap-2">
            <UCheckbox
              v-model="setAsDefault"
              :label="t('setAsDefault')"
            />
            <UiButton
              icon="i-lucide-save"
              :label="t('savePreset')"
              color="neutral"
              variant="outline"
              type="button"
              :loading="savingPreset"
              :disabled="!presetName.trim()"
              @click="onSavePreset"
            />
          </div>
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-end gap-2 w-full">
        <UiButton
          :label="t('cancel')"
          color="neutral"
          variant="ghost"
          type="button"
          @click="open = false"
        />
        <UiButton
          :label="t('use')"
          color="primary"
          type="button"
          :disabled="!preview"
          @click="applyPreview"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import {
  usePasswordGenerator,
  type IPasswordConfig,
} from '~/composables/passwords/useGenerator'
import {
  usePasswordGeneratorPresets,
  type PasswordGeneratorPreset,
} from '~/composables/passwords/useGeneratorPresets'

const value = defineModel<string>('value', { default: '' })
const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
const toast = useToast()
const { generate } = usePasswordGenerator()
const { copy, copied } = useClipboard({ copiedDuring: 1500 })
const presetStore = usePasswordGeneratorPresets()

const patternHelpKeys = ['c', 'C', 'v', 'V', 'd', 'a', 'A', 's'] as const

const config = reactive<IPasswordConfig>({
  length: 20,
  uppercase: true,
  lowercase: true,
  numbers: true,
  symbols: true,
  excludeChars: '',
  usePattern: false,
  pattern: 'cvcv-cvcv-cvcv',
})

const preview = ref('')
const presets = ref<PasswordGeneratorPreset[]>([])
const selectedPresetId = ref<string | undefined>(undefined)
const presetName = ref('')
const setAsDefault = ref(false)
const savingPreset = ref(false)

const presetItems = computed(() =>
  presets.value.map((p) => ({
    label: p.isDefault ? `${p.name} ★` : p.name,
    value: p.id,
  })),
)

const regenerate = () => {
  preview.value = generate(config)
}

// Regenerate whenever inputs change.
watch(
  () => [
    config.length,
    config.uppercase,
    config.lowercase,
    config.numbers,
    config.symbols,
    config.excludeChars,
    config.usePattern,
    config.pattern,
  ],
  regenerate,
  { immediate: false },
)

const applyPreset = (preset: PasswordGeneratorPreset) => {
  config.length = preset.length
  config.uppercase = preset.uppercase
  config.lowercase = preset.lowercase
  config.numbers = preset.numbers
  config.symbols = preset.symbols
  config.excludeChars = preset.excludeChars ?? ''
  config.usePattern = preset.usePattern
  config.pattern = preset.pattern ?? ''
  selectedPresetId.value = preset.id
  presetName.value = preset.name
  setAsDefault.value = preset.isDefault
}

const onPresetSelected = (id: unknown) => {
  if (typeof id !== 'string') return
  const preset = presets.value.find((p) => p.id === id)
  if (preset) applyPreset(preset)
}

const loadPresetsAsync = async () => {
  try {
    presets.value = await presetStore.getAllAsync()
  } catch (error) {
    console.error('[Generator] Failed to load presets:', error)
  }
}

const onSavePreset = async () => {
  if (savingPreset.value) return
  const name = presetName.value.trim()
  if (!name) return
  const payload = {
    name,
    length: config.length,
    uppercase: config.uppercase,
    lowercase: config.lowercase,
    numbers: config.numbers,
    symbols: config.symbols,
    excludeChars: config.excludeChars,
    usePattern: config.usePattern,
    pattern: config.pattern,
    isDefault: setAsDefault.value,
  }
  savingPreset.value = true
  try {
    // Update when a preset with the same name already exists (name is the
    // effective unique key); otherwise create a new one.
    const existing = presets.value.find((p) => p.name === name)
    if (existing) {
      await presetStore.updateAsync(existing.id, payload)
      selectedPresetId.value = existing.id
    } else {
      const id = await presetStore.createAsync(payload)
      selectedPresetId.value = id
    }
    await loadPresetsAsync()
    toast.add({ title: t('toast.presetSaved'), color: 'success' })
  } catch (error) {
    console.error('[Generator] Failed to save preset:', error)
    toast.add({
      title: t('toast.presetSaveError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  } finally {
    savingPreset.value = false
  }
}

const onDeletePreset = async () => {
  const id = selectedPresetId.value
  if (!id) return
  try {
    await presetStore.deleteAsync(id)
    selectedPresetId.value = undefined
    presetName.value = ''
    setAsDefault.value = false
    await loadPresetsAsync()
    toast.add({ title: t('toast.presetDeleted'), color: 'success' })
  } catch (error) {
    console.error('[Generator] Failed to delete preset:', error)
    toast.add({
      title: t('toast.presetDeleteError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

// On drawer open: load presets, apply default if any, then generate.
watch(open, async (isOpen) => {
  if (!isOpen) return
  await loadPresetsAsync()
  const defaultPreset = await presetStore.getDefaultAsync()
  if (defaultPreset) {
    applyPreset(defaultPreset)
  }
  regenerate()
})

const copyPreview = () => {
  if (preview.value) void copy(preview.value)
}

const applyPreview = () => {
  if (!preview.value) return
  value.value = preview.value
  open.value = false
}
</script>

<i18n lang="yaml">
de:
  title: Passwort generieren
  preview: Vorschau
  regenerate: Neu generieren
  copy: Kopieren
  copied: Kopiert
  length: Länge
  characterTypes: Zeichentypen
  excludeChars: Zeichen ausschließen
  excludeCharsPlaceholder: z.B. äöüIlO01
  usePattern: Pattern-Modus
  patternHelpToggle: Pattern-Hilfe anzeigen
  pattern: Pattern
  patternPlaceholder: z.B. cvcv-cvcv-cvcv
  patternHelpTitle: Pattern-Zeichen
  patternHelp:
    c: Kleinbuchstaben-Konsonant
    C: Großbuchstaben-Konsonant
    v: Kleinbuchstaben-Vokal
    V: Großbuchstaben-Vokal
    d: Ziffer (0-9)
    a: beliebiger Kleinbuchstabe
    A: beliebiger Großbuchstabe
    s: Sonderzeichen
    other: Andere Zeichen werden als Literale übernommen (-, _, ., …)
  loadPreset: Preset laden
  selectPreset: Preset auswählen
  deletePreset: Preset löschen
  saveAsPreset: Als Preset speichern
  presetNamePlaceholder: Name des Presets
  setAsDefault: Als Standard
  savePreset: Speichern
  toast:
    presetSaved: Preset gespeichert
    presetSaveError: Preset konnte nicht gespeichert werden
    presetDeleted: Preset gelöscht
    presetDeleteError: Preset konnte nicht gelöscht werden
  cancel: Abbrechen
  use: Übernehmen

en:
  title: Generate password
  preview: Preview
  regenerate: Regenerate
  copy: Copy
  copied: Copied
  length: Length
  characterTypes: Character types
  excludeChars: Exclude characters
  excludeCharsPlaceholder: e.g. IlO01
  usePattern: Pattern mode
  patternHelpToggle: Show pattern help
  pattern: Pattern
  patternPlaceholder: e.g. cvcv-cvcv-cvcv
  patternHelpTitle: Pattern characters
  patternHelp:
    c: lowercase consonant
    C: uppercase consonant
    v: lowercase vowel
    V: uppercase vowel
    d: digit (0-9)
    a: any lowercase letter
    A: any uppercase letter
    s: symbol
    other: Other characters are kept as literals (-, _, ., …)
  loadPreset: Load preset
  selectPreset: Select preset
  deletePreset: Delete preset
  saveAsPreset: Save as preset
  presetNamePlaceholder: Preset name
  setAsDefault: Set as default
  savePreset: Save
  toast:
    presetSaved: Preset saved
    presetSaveError: Failed to save preset
    presetDeleted: Preset deleted
    presetDeleteError: Failed to delete preset
  cancel: Cancel
  use: Use
</i18n>
