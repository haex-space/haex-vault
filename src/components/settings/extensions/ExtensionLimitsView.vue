<script setup lang="ts">
/**
 * The limits of one extension (spec 017, FR-031, T092): rows, parallel requests, size of a
 * statement, run time and size of an answer, and the run time of a model job (spec 055). A value
 * is saved when its field loses focus or on Enter, for every own device (spec 023 FR-021); one
 * outside its bounds is refused with the reason and the stored value comes back.
 */
import { invoke } from '@tauri-apps/api/core'
import type { ExtensionLimits } from '@bindings/ExtensionLimits'
import type { ExtensionLimitsView } from '@bindings/ExtensionLimitsView'

const props = defineProps<{ extensionId: string }>()
const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()

type Field = keyof ExtensionLimits

/** The fields in the order shown; their bounds come from holzi, which alone checks them. */
const FIELDS: Field[] = [
  'maxRows',
  'maxConcurrent',
  'maxSqlBytes',
  'timeoutMs',
  'maxResponseBytes',
  'modelJobTimeoutMs',
]

const view = ref<ExtensionLimitsView | null>(null)
const stored = computed(() => view.value?.values ?? null)
const draft = reactive<Partial<Record<Field, string>>>({})
const failure = ref<string | null>(null)
const busy = ref(false)

function showStored() {
  for (const key of FIELDS)
    draft[key] = stored.value ? String(stored.value[key]) : ''
}

/** Reads the stored limits into the fields; a refusal of the last change stays shown. */
async function loadAsync() {
  try {
    view.value = await invoke<ExtensionLimitsView>('extension_limits_get', {
      extensionId: props.extensionId,
    })
  } catch (error) {
    failure.value = errString(error)
  }
  showStored()
}

async function commitAsync(key: Field) {
  const current = stored.value
  if (!current || busy.value) return
  const text = String(draft[key] ?? '').trim()
  const value = Number(text)
  // An emptied field or no whole number is not a limit: the stored one comes back.
  if (text === '' || !Number.isSafeInteger(value)) {
    draft[key] = String(current[key])
    failure.value = null
    return
  }
  if (value === current[key]) return
  busy.value = true
  try {
    await invoke('extension_limits_set', {
      extensionId: props.extensionId,
      limits: { ...current, [key]: value },
    })
    failure.value = null
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
  await loadAsync()
}

onMounted(loadAsync)
</script>

<template>
  <SettingsGroup :label="t('settings.extensions.limits.title')">
    <SettingsRow
      v-for="key in FIELDS"
      :key="key"
      :title="t(`settings.extensions.limits.${key}`)"
      :description="
        view
          ? t('settings.extensions.limits.range', {
              min: view.min[key].toLocaleString(),
              max: view.max[key].toLocaleString(),
            })
          : undefined
      "
      :label-for="`extension-limit-${key}`"
    >
      <div class="w-40 max-w-full">
        <UiInput
          :id="`extension-limit-${key}`"
          v-model="draft[key]"
          type="number"
          inputmode="numeric"
          :labels="fieldLabels.input.value"
          :disabled="busy || !stored"
          :data-testid="`extension-limit-${key}`"
          @blur="commitAsync(key)"
          @keydown.enter.prevent="commitAsync(key)"
        />
      </div>
    </SettingsRow>
    <li v-if="failure" class="px-4 py-3 text-sm text-destructive" role="alert">
      {{ failure }}
    </li>
  </SettingsGroup>
</template>
