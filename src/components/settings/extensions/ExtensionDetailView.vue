<script setup lang="ts">
import { statusErrorKey, statusKey } from '~/lib/extensions/apps'
import type { DeviceState } from '@bindings/DeviceState'

/**
 * One extension in the settings (spec 017, US3, T070): what it is and its state on this device,
 * its state on each own device (US4, T080), managing it and its limits (US7, T092), its
 * permissions and the models it may use (spec 055), then its log on this device (US5, T087). A
 * removed extension whose data was kept shows that data instead. Reached from its row in
 * "Erweiterungen".
 */
const { t, te } = useI18n()
const router = useTabRouter()
const store = useExtensionsStore()

const extensionId = computed(() => router.route.params.extensionId ?? '')
const extension = computed(() =>
  store.list.find((e) => e.id === extensionId.value),
)

/** A state with its error kind, e.g. "Datenbank-Änderung fehlgeschlagen · Migration fehlt". */
function stateText(status: string, error?: string | null): string {
  const text = t(statusKey(status))
  const errorKey = statusErrorKey(status, error)
  if (!errorKey) return text
  return `${text} · ${te(errorKey) ? t(errorKey) : error}`
}

/**
 * A device's state and the version it runs; a device with an older version than this one says
 * that some of its data waits for its update (research R10).
 */
function deviceText(device: DeviceState): string {
  const state = stateText(device.status, device.error)
  if (!device.version) return state
  const key = device.behind
    ? 'settings.extensions.deviceBehind'
    : 'settings.extensions.deviceVersion'
  return `${state} · ${t(key, { version: device.version })}`
}

function deviceTitle(device: DeviceState): string {
  const name = device.deviceName || t('settings.extensions.unnamedDevice')
  return device.thisDevice
    ? `${name} (${t('settings.extensions.thisDevice')})`
    : name
}
</script>

<template>
  <section class="flex flex-col gap-3">
    <p v-if="!extension" class="px-1 text-sm text-muted-foreground">
      {{ t('errors.extensions.notFound') }}
    </p>
    <template v-else>
      <SettingsGroup>
        <SettingsRow
          :title="extension.title"
          :description="extension.description"
        >
          <template #title>
            <span class="flex items-center gap-3">
              <img
                v-if="store.icons[extension.id]"
                :src="store.icons[extension.id]"
                alt=""
                class="size-6 object-contain"
              />
              <Icon v-else name="lucide:puzzle" class="size-6" />
              <span class="font-semibold">{{ extension.title }}</span>
            </span>
          </template>
        </SettingsRow>
        <SettingsRow
          :title="t('settings.extensions.versionLabel')"
          :description="extension.version ?? '—'"
        />
        <SettingsRow
          :title="t('settings.extensions.publisher')"
          :description="extension.publisherFingerprint"
        />
        <SettingsRow
          v-if="extension.state === 'installed'"
          :title="t('settings.extensions.stateHere')"
          :description="
            extension.statusHere
              ? stateText(extension.statusHere, extension.statusErrorHere)
              : t('settings.extensions.notStartedHere')
          "
        />
      </SettingsGroup>
      <SettingsGroup
        v-if="extension.devices.length > 0"
        :label="t('settings.extensions.devices')"
      >
        <SettingsRow
          v-for="device in extension.devices"
          :key="device.deviceId"
          :title="deviceTitle(device)"
          :description="deviceText(device)"
          data-testid="extension-device-state"
        />
      </SettingsGroup>
      <template v-if="extension.state === 'installed'">
        <SettingsExtensionsExtensionManageView :extension="extension" />
        <SettingsExtensionsExtensionLimitsView :extension-id="extension.id" />
        <SettingsExtensionsExtensionPermissionsView
          :extension-id="extension.id"
        />
        <SettingsExtensionsExtensionModelsView :extension-id="extension.id" />
        <SettingsExtensionsExtensionLogsView :extension-id="extension.id" />
      </template>
      <SettingsExtensionsExtensionKeptDataView v-else :extension="extension" />
    </template>
  </section>
</template>
