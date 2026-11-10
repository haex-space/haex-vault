<script setup lang="ts">
/**
 * "Gerät entfernen" on a main device (spec 024, user story 6, FR-026, FR-028, FR-035): explains
 * what removing does before anything happens — the device gets no new data and cannot read new
 * changes, what is on it stays and only the passphrase protects it, there is no remote wipe, and it
 * comes back only by linking it again. For a main device it also says this only helps against an
 * honest device. The device comes from the location; this device itself is never offered.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useSyncDevicesStore()
const removeDevice = useActionOrThrow('settings.devices.remove')

const devicePubkey = computed(() => router.route.params.devicePubkey ?? '')
const device = computed(() =>
  store.devices.find((entry) => entry.devicePubkey === devicePubkey.value),
)
const busy = ref(false)

onMounted(async () => {
  await store.startListening()
  await store.loadAsync()
})

onBeforeUnmount(() => {
  store.stopListening()
})

/** A device that is gone, or this device, has nothing to remove here. */
const missing = computed(
  () => !store.loading && (!device.value || device.value.isCurrent),
)

/**
 * Removes the selected device, ignoring calls while removal is pending.
 * Returns to the previous view on success or shows the removal error as a toast.
 */
async function onRemove() {
  if (!device.value || busy.value) return
  busy.value = true
  try {
    await removeDevice({ devicePubkey: device.value.devicePubkey })
    router.back()
  } catch (e) {
    const kind = (e as { kind?: string })?.kind
    toast.error(
      kind === 'NotMainDevice' ? t('settings.remove.notMain') : errString(e),
    )
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="remove-device-view">
    <p
      v-if="missing"
      class="text-sm text-muted-foreground"
      role="status"
      data-testid="remove-device-missing"
    >
      {{ t('settings.remove.missing') }}
    </p>

    <template v-else-if="device">
      <SettingsGroup
        :label="
          t('settings.remove.title', {
            name: device.alias ?? t('settings.federation.unnamed'),
          })
        "
      >
        <li class="flex flex-col gap-2 px-4 py-4 text-sm">
          <p>{{ t('settings.remove.consequences') }}</p>
          <ul class="list-disc space-y-1 pl-5">
            <li>{{ t('settings.remove.noNewData') }}</li>
            <li>{{ t('settings.remove.staysOnDevice') }}</li>
            <li>{{ t('settings.remove.noRemoteWipe') }}</li>
            <li>{{ t('settings.remove.linkAgain') }}</li>
          </ul>
        </li>
      </SettingsGroup>
      <p
        v-if="device.role === 'main'"
        class="px-1 text-sm text-destructive"
        role="note"
        data-testid="remove-device-main-warning"
      >
        {{ t('settings.remove.mainWarning') }}
      </p>
      <div class="flex gap-2">
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="remove-device-confirm"
          @click="onRemove"
        >
          {{ t('settings.remove.confirm') }}
        </UiButton>
        <UiButton
          variant="outline"
          :disabled="busy"
          data-testid="remove-device-cancel"
          @click="router.back()"
        >
          {{ t('settings.remove.cancel') }}
        </UiButton>
      </div>
    </template>
  </section>
</template>
