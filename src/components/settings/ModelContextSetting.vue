<script setup lang="ts">
/**
 * The context of one local model on this device (spec 086 US2): its length (Auto, a preset or a typed
 * number) and the type of its KV cache, with what the model was trained for, what the device allows
 * and what is in effect. A choice is kept as soon as it is made, with no save button; a typed length
 * the rules forbid says why under its field, any other failure is a toast. The sentences come from
 * what the backend reports about the choice (`settings.models.context.report`), which uses the same
 * planner as the load. A choice for the loaded model is reported to the caller, which loads the model
 * again (the setting applies at the next load).
 */
import { toast } from 'vue-sonner'

import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import {
  CONTEXT_CACHES,
  CONTEXT_PRESETS,
  DEFAULT_CONTEXT_SETTING,
  isDefaultSetting,
  isRefused,
  noteKeys,
  parseTokens,
  presetsFor,
  reasonKey,
  tokensOf,
  verdictKey,
  type ContextCache,
  type ContextReport,
  type ContextSetting,
} from '~/lib/models/contextSetting'

const props = defineProps<{
  modelId: string
  /** The model is the loaded one. */
  active: boolean
  /** The model is being loaded, unloaded or deleted. */
  busy: boolean
}>()

const emit = defineEmits<{
  /** The saved model changed: the parent reloads it if it is still active. */
  changed: [modelId: string]
}>()

const { t, locale } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const getContext = useActionOrThrow('settings.models.context.get')
const setContext = useActionOrThrow('settings.models.context.set')
const reportContext = useActionOrThrow('settings.models.context.report')

const OTHER = 'other'

const setting = ref<ContextSetting>({ ...DEFAULT_CONTEXT_SETTING })
const report = ref<ContextReport | null>(null)
const saving = ref(false)
const loading = ref(true)
let selection = 0
let reportRequest = 0
let pendingSaves = 0
const reloadModels = new Set<string>()
let saveQueue = Promise.resolve()
/** The field for a typed length is open. */
const typing = ref(false)
const typed = ref('')
const typedError = ref<string | undefined>()

const format = (count: number) =>
  new Intl.NumberFormat(locale.value).format(count)
const tokens = (count: number) =>
  t('models.context.tokens', { count: format(count) })

function sentence(key: string, r: ContextReport | null = report.value) {
  return t(key, {
    need: r?.needMib != null ? format(r.needMib) : '',
    free: r?.freeMib != null ? format(r.freeMib) : '',
  })
}

async function reportAsync(
  next: ContextSetting,
  modelId = props.modelId,
): Promise<ContextReport> {
  return (await reportContext({
    modelId,
    ...next,
  })) as ContextReport
}

async function refreshAsync() {
  const modelId = props.modelId
  const current = selection
  const request = ++reportRequest
  try {
    const next = await reportAsync(setting.value, modelId)
    if (current === selection && request === reportRequest) report.value = next
  } catch (e) {
    if (current === selection && request === reportRequest)
      toast.error(`${t('models.context.title')}: ${errString(e)}`)
  }
}

async function loadAsync() {
  const modelId = props.modelId
  const current = ++selection
  ++reportRequest
  loading.value = true
  report.value = null
  setting.value = { ...DEFAULT_CONTEXT_SETTING }
  typing.value = false
  typed.value = ''
  typedError.value = undefined
  try {
    // Returning to a model must read after an already-running write for it finishes.
    await saveQueue
    if (current !== selection) return
    const kept = (await getContext({ modelId })) as ContextSetting
    if (current !== selection) return
    setting.value = kept
    const length = tokensOf(kept)
    typing.value = length !== null && !CONTEXT_PRESETS.includes(length)
    typed.value = typing.value && length !== null ? String(length) : ''
    loading.value = false
    await refreshAsync()
  } catch (e) {
    if (current === selection)
      toast.error(`${t('models.context.title')}: ${errString(e)}`)
  }
}

watch(() => props.modelId, loadAsync, { immediate: true, flush: 'sync' })
// What is in effect changes when the model is loaded or unloaded, or its load ends.
watch(
  () => [props.active, props.busy],
  ([, busy], [, wasBusy]) => {
    if (!loading.value && (!busy || wasBusy === busy)) void refreshAsync()
  },
)

async function saveAsync(next: ContextSetting, validateTyped = false) {
  if (loading.value) return
  const modelId = props.modelId
  const current = selection
  ++pendingSaves
  saving.value = true
  // Validation and persistence share one queue: a later committed value cannot be dropped or
  // overtaken by an earlier report. A selection change cancels work that has not been written.
  const operation = saveQueue.then(async () => {
    try {
      if (current !== selection) return
      ++reportRequest
      if (validateTyped) {
        const checked = await reportAsync(next, modelId)
        if (current !== selection) return
        if (isRefused(checked.verdict)) {
          typedError.value = sentence(verdictKey(checked.verdict), checked)
          return
        }
      }
      const kept = (await setContext({ modelId, ...next })) as ContextSetting
      // Keep successful writes even if selection changes or reporting fails.
      reloadModels.add(modelId)
      if (current !== selection) return
      setting.value = kept
      typedError.value = undefined
      await refreshAsync()
    } catch (e) {
      if (current !== selection) return
      toast.error(`${t('models.context.title')}: ${errString(e)}`)
      await refreshAsync()
    } finally {
      saving.value = --pendingSaves > 0
      if (!saving.value) {
        // Reload once with the last persisted choice, without overlapping model loads.
        for (const id of reloadModels) emit('changed', id)
        reloadModels.clear()
      }
    }
  })
  saveQueue = operation
  await operation
}

const contextValue = computed(() => {
  if (typing.value) return OTHER
  return setting.value.context === 'auto' ? 'auto' : setting.value.context
})

const contextOptions = computed<SettingsSelectOption[]>(() => [
  { value: 'auto', label: t('models.context.auto') },
  ...presetsFor(report.value).map((count) => ({
    value: String(count),
    label: tokens(count),
  })),
  { value: OTHER, label: t('models.context.other') },
])

const cacheOptions = computed<SettingsSelectOption[]>(() =>
  CONTEXT_CACHES.map((cache) => ({
    value: cache,
    label: t(`models.context.cacheOption.${cache}`),
  })),
)

function pickContext(value: string) {
  typedError.value = undefined
  if (value === OTHER) {
    typing.value = true
    typed.value = tokensOf(setting.value)?.toString() ?? ''
    return
  }
  typing.value = false
  void saveAsync({ ...setting.value, context: value })
}

function pickCache(value: string) {
  void saveAsync({ ...setting.value, cache: value as ContextCache })
}

/** A typed length is kept when it is a number the rules allow; else the field says why. */
async function commitTypedAsync() {
  const length = parseTokens(typed.value)
  if (length === null) {
    typedError.value = t('models.context.invalid')
    return
  }
  if (!saving.value && String(length) === setting.value.context) {
    typedError.value = undefined
    return
  }
  typedError.value = undefined
  await saveAsync({ ...setting.value, context: String(length) }, true)
}

function reset() {
  typing.value = false
  typedError.value = undefined
  void saveAsync({ ...DEFAULT_CONTEXT_SETTING })
}

const verdictText = computed(() =>
  report.value ? sentence(verdictKey(report.value.verdict)) : '',
)
const verdictBad = computed(
  () =>
    report.value !== null &&
    (isRefused(report.value.verdict) || report.value.verdict === 'too-large'),
)
const notes = computed(() =>
  noteKeys(setting.value, report.value).map((key) => t(key)),
)

const trainedText = computed(() =>
  report.value?.trained != null
    ? tokens(report.value.trained)
    : t('models.context.unknown'),
)

const allowedText = computed(() => {
  const r = report.value
  if (!r || !r.sized) return t('models.context.unknown')
  const gpu = r.maxOnGpu
  const any = r.maxAny
  if (gpu === null && any === null) return t('models.context.allowedNone')
  if (gpu === null)
    return t('models.context.allowedAnyOnly', { any: tokens(any ?? 0) })
  if (gpu === any || any === null)
    return t('models.context.allowedGpuOnly', { gpu: tokens(gpu) })
  return t('models.context.allowedValue', {
    gpu: tokens(gpu),
    any: tokens(any),
  })
})

const effective = computed(() => report.value?.effective ?? null)
const effectiveText = computed(() => {
  const e = effective.value
  if (!e) return props.active ? '' : t('models.context.notLoaded')
  const layers =
    e.gpuLayers > e.layers
      ? t('models.context.layersAll')
      : t('models.context.layersSome', { on: e.gpuLayers, all: e.layers })
  const stretched = e.yarnFrom
    ? `, ${t('models.context.stretched', { trained: format(e.yarnFrom) })}`
    : ''
  return `${t('models.context.effectiveValue', { count: format(e.context), layers, cache: t(`models.context.cacheOption.${e.cache}`) })}${stretched}`
})
</script>

<template>
  <SettingsGroup :label="t('models.context.title')">
    <SettingsRow
      :title="t('models.context.length')"
      :description="t('models.context.lengthHint')"
      label-for="model-context-length"
    >
      <SettingsSelect
        id="model-context-length"
        class="w-44"
        :model-value="contextValue"
        :options="contextOptions"
        :disabled="loading || saving || busy"
        data-testid="model-context-length"
        @update:model-value="pickContext"
      />
      <div v-if="typing" class="w-44 max-w-full">
        <UiInput
          v-model="typed"
          type="text"
          inputmode="numeric"
          :placeholder="t('models.context.custom')"
          :error="typedError"
          :labels="fieldLabels.input.value"
          :disabled="loading || saving || busy"
          data-testid="model-context-typed"
          @input="typedError = undefined"
          @blur="commitTypedAsync"
          @keydown.enter.prevent="commitTypedAsync"
        />
      </div>
      <UiButton
        v-if="!isDefaultSetting(setting)"
        type="button"
        size="sm"
        variant="outline"
        :disabled="loading || saving || busy"
        data-testid="model-context-reset"
        @click="reset"
      >
        {{ t('models.context.reset') }}
      </UiButton>
      <template v-if="verdictText || notes.length" #below>
        <p
          class="text-sm"
          :class="verdictBad ? 'text-destructive' : 'text-muted-foreground'"
          data-testid="model-context-verdict"
        >
          {{ verdictText }}
        </p>
        <p
          v-for="note in notes"
          :key="note"
          class="mt-1 text-sm text-muted-foreground"
        >
          {{ note }}
        </p>
      </template>
    </SettingsRow>
    <SettingsRow
      :title="t('models.context.cache')"
      :description="t('models.context.cacheHint')"
      label-for="model-context-cache"
    >
      <SettingsSelect
        id="model-context-cache"
        class="w-44"
        :model-value="setting.cache"
        :options="cacheOptions"
        :disabled="loading || saving || busy"
        data-testid="model-context-cache"
        @update:model-value="pickCache"
      />
    </SettingsRow>
    <SettingsRow :title="t('models.context.trained')">
      <span class="text-sm" data-testid="model-context-trained">{{
        trainedText
      }}</span>
    </SettingsRow>
    <SettingsRow :title="t('models.context.allowed')">
      <span class="text-sm" data-testid="model-context-allowed">{{
        allowedText
      }}</span>
    </SettingsRow>
    <SettingsRow
      :title="t('models.context.effective')"
      :description="effective ? t(reasonKey(effective.reason)) : undefined"
    >
      <span class="text-sm" data-testid="model-context-effective">{{
        effectiveText
      }}</span>
    </SettingsRow>
  </SettingsGroup>
</template>
