<script setup lang="ts">
/**
 * A switch of the chat settings (spec 072 FR-080): saved as soon as it flips, without a save
 * button (spec 023 FR-021). `scope` says whether the value is the vault's (`vault`, every device)
 * or this device's (`device`). A missing value reads as on, like the backend does; the backend
 * accepts only `true` and `false`.
 */
import { toast } from 'vue-sonner'
import { useSettingsDevice } from '~/components/settings/deviceContext'
import type { PrefScope } from '~/composables/usePreferences'

const props = defineProps<{
  prefKey: string
  scope: 'vault' | 'device'
  title: string
  description: string
  testId: string
}>()

const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const device = props.scope === 'device' ? useSettingsDevice() : null

const prefScope = computed<PrefScope>(() =>
  device
    ? { kind: 'device', uuid: device.info.value.vaultDeviceUuid }
    : { kind: 'vault' },
)

const on = ref(true)
const busy = ref(false)
let reloadPending = false

async function loadAsync() {
  try {
    on.value = (await getPrefAsync(prefScope.value, props.prefKey)) !== 'false'
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function toggleAsync(next: boolean) {
  if (busy.value) return
  const previous = on.value
  on.value = next
  busy.value = true
  try {
    // action-exempt: whether this vault sends read receipts, shows notifications or takes
    // requests of strangers is the user's own choice; specs 072 and 099 give agents no action for it.
    await setPrefAsync(prefScope.value, props.prefKey, String(next))
  } catch (cause) {
    on.value = previous
    toast.error(errString(cause))
  } finally {
    while (reloadPending) {
      reloadPending = false
      await loadAsync()
    }
    busy.value = false
  }
}

onMounted(loadAsync)
onVaultTablesChanged(['preferences'], () => {
  if (busy.value) {
    reloadPending = true
    return
  }
  return loadAsync()
})
</script>

<template>
  <SettingsRow
    :title="title"
    :description="description"
    :label-for="`${testId}-switch`"
  >
    <ShadcnSwitch
      :id="`${testId}-switch`"
      :model-value="on"
      :disabled="busy"
      :data-testid="testId"
      @update:model-value="toggleAsync($event === true)"
    />
  </SettingsRow>
</template>
