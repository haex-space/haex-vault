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
  /** The setting of the loaded model changed: it applies after a reload. */
  changed: []
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

async function reportAsync(next: ContextSetting): Promise<ContextReport> {
  return (await reportContext({
    modelId: props.modelId,
    ...next,
  })) as ContextReport
}

async function refreshAsync() {
  const modelId = props.modelId
  try {
    const next = await reportAsync(setting.value)
    if (modelId === props.modelId) report.value = next
  } catch (e) {
    toast.error(`${t('models.context.title')}: ${errString(e)}`)
  }
}

async function loadAsync() {
  const modelId = props.modelId
  report.value = null
  typedError.value = undefined
  try {
    const kept = (await getContext({ modelId })) as ContextSetting
    if (modelId !== props.modelId) return
    setting.value = kept
    const length = tokensOf(kept)
    typing.value = length !== null && !CONTEXT_PRESETS.includes(length)
    typed.value = typing.value && length !== null ? String(length) : ''
    await refreshAsync()
  } catch (e) {
    toast.error(`${t('models.context.title')}: ${errString(e)}`)
  }
}

watch(() => props.modelId, loadAsync, { immediate: true })
// What is in effect changes when the model is loaded or unloaded, or its load ends.
watch(
  () => [props.active, props.busy],
  ([, busy], [, wasBusy]) => {
    if (!busy || wasBusy === busy) void refreshAsync()
  },
)

async function saveAsync(next: ContextSetting) {
  if (saving.value) return
  saving.value = true
  try {
    setting.value = (await setContext({
      modelId: props.modelId,
      ...next,
    })) as ContextSetting
    report.value = await reportAsync(setting.value)
    if (props.active) emit('changed')
  } catch (e) {
    toast.error(`${t('models.context.title')}: ${errString(e)}`)
    await refreshAsync()
  } finally {
    saving.value = false
  }
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
  if (String(length) === setting.value.context) {
    typedError.value = undefined
    return
  }
  const next = { ...setting.value, context: String(length) }
  try {
    const checked = await reportAsync(next)
    if (isRefused(checked.verdict)) {
      typedError.value = sentence(verdictKey(checked.verdict), checked)
      return
    }
  } catch (e) {
    toast.error(`${t('models.context.title')}: ${errString(e)}`)
    return
  }
  typedError.value = undefined
  await saveAsync(next)
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
  return `${t('models.context.effectiveValue', { count: format(e.context), layers })}${stretched}`
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
        :disabled="saving || busy"
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
          :disabled="saving || busy"
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
        :disabled="saving || busy"
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
        :disabled="saving || busy"
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
