<script setup lang="ts">
/**
 * The category "Föderation" (spec 023-settings-app, FR-022; spec 024, user story 4, FR-033 to
 * FR-035, FR-046): the vault's public identity, the devices with role and online state, and the
 * servers they find each other through. The list reloads on `sync-devices-changed`, so a device
 * coming online, a rename, or a new device show without reloading. Only a main device links,
 * admits and removes devices (FR-035); any other says so instead of offering it.
 */
import { canManageDevices } from '~/lib/sync/deviceStatus'

const { t } = useI18n()
const store = useSyncDevicesStore()

const now = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | undefined

const manages = computed(() =>
  canManageDevices(store.status?.thisDevice ?? null),
)
const ownSchema = computed(
  () => store.devices.find((device) => device.isCurrent)?.schema ?? null,
)
const notice = computed(() => {
  const place = store.status?.thisDevice
  return place === 'awaiting_admission' || place === 'removed' ? place : null
})

onMounted(async () => {
  // Refreshes the "last online" ages now and then.
  ticker = setInterval(() => (now.value = Date.now()), 30_000)
  await store.startListening()
  await Promise.all([store.loadAsync(), store.loadIdentityAsync()])
})

onBeforeUnmount(() => {
  clearInterval(ticker)
  store.stopListening()
})
</script>

<template>
  <section class="flex flex-col gap-3">
    <div v-if="store.loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <p v-if="store.error" class="text-sm text-destructive" role="alert">
      {{ t('settings.federation.loadFailed') }}: {{ store.error }}
    </p>

    <p
      v-if="notice"
      class="rounded-xl bg-muted px-4 py-3 text-sm"
      role="status"
      data-testid="settings-federation-notice"
    >
      {{ t(`settings.federation.notice.${notice}`) }}
    </p>

    <SettingsVaultIdentityRow
      v-if="store.identity"
      :identity="store.identity"
    />

    <SettingsAdmissionRequests v-if="manages" />

    <SettingsGroup v-if="manages">
      <SettingsRow
        to="/federation/devices/link"
        icon="lucide:qr-code"
        :title="t('settings.locations.federation.link.title')"
        :description="t('settings.locations.federation.link.description')"
        data-testid="settings-link-device"
      />
    </SettingsGroup>
    <p
      v-else-if="store.status && !notice"
      class="px-1 text-sm text-muted-foreground"
      data-testid="settings-main-only"
    >
      {{ t('settings.federation.mainOnly') }}
    </p>

    <SettingsGroup
      v-if="!store.loading && !store.error"
      :label="t('settings.federation.devicesLabel')"
    >
      <SettingsDeviceRow
        v-for="device in store.devices"
        :key="device.vaultDeviceUuid"
        :device="device"
        :now="now"
        :own="ownSchema"
        :removable="manages"
      />
    </SettingsGroup>

    <SettingsSyncServersGroup />
  </section>
</template>
