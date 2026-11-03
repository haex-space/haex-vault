<script setup lang="ts">
/**
 * This device's name (spec 002 US3). Saved when the field loses focus or on Enter, without a
 * save button (spec 023 FR-021); an empty name is not saved and the field says why. A failed save
 * shows as a toast.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const setAlias = useActionOrThrow('settings.device.setAlias')

const props = defineProps<{
  currentAlias: string
}>()

const emit = defineEmits<{
  saved: [alias: string]
}>()

const localValue = ref(props.currentAlias)
const showRequired = ref(false)
const busy = ref(false)

watch(
  () => props.currentAlias,
  (v) => {
    localValue.value = v
  },
)

/** Hides the "required" hint as soon as the operator types. */
function onInput() {
  showRequired.value = false
}

async function commitAsync() {
  const trimmed = localValue.value.trim()
  if (!trimmed) {
    showRequired.value = true
    return
  }
  if (trimmed === props.currentAlias || busy.value) return
  busy.value = true
  try {
    await setAlias({ alias: trimmed })
    localValue.value = trimmed
    emit('saved', trimmed)
  } catch (e) {
    toast.error(`${t('errors.aliasSaveFailed')}: ${errString(e)}`)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <SettingsGroup>
    <SettingsRow
      :title="t('settings.alias.label')"
      label-for="settings-alias-input"
    >
      <div class="w-56 max-w-full">
        <UiInput
          id="settings-alias-input"
          v-model="localValue"
          type="text"
          :error="
            showRequired && !localValue.trim()
              ? t('settings.alias.required')
              : undefined
          "
          :labels="fieldLabels.input.value"
          :disabled="busy"
          data-testid="settings-alias"
          @input="onInput"
          @blur="commitAsync"
          @keydown.enter.prevent="commitAsync"
        />
      </div>
    </SettingsRow>
  </SettingsGroup>
</template>
