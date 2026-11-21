<script setup lang="ts">
/**
 * "Allgemein → Grundeinstellung" (spec 023-settings-app FR-005, spec 042): language, the row to
 * the vault-password view, device name, session restore and, where the device has it, the screen
 * capture protection (spec 043 FR-011a).
 */
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t } = useI18n()
const device = useSettingsDevice()
const { capabilities } = useDeviceCapabilities()
</script>

<template>
  <div class="flex flex-col gap-6">
    <SettingsLanguageSetting />
    <SettingsGroup>
      <SettingsRow
        to="/general/basic/password"
        icon="lucide:key-round"
        :title="t('settings.locations.general.basic.password.title')"
        :description="
          t('settings.locations.general.basic.password.description')
        "
        data-testid="settings-row-general.basic.password"
      />
    </SettingsGroup>
    <SettingsAliasSetting
      :current-alias="device.info.value.alias ?? ''"
      @saved="device.reloadAsync"
    />
    <SettingsSessionRestoreSetting />
    <SettingsScreenCaptureSetting v-if="capabilities?.screenCapture" />
    <SettingsDiagnosticLogSetting />
  </div>
</template>
