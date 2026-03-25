<template>
  <div class="h-full">
    <HaexSystemSettingsDevicesCurrent v-if="activeView === 'current'" @back="goBack" />
    <HaexSystemSettingsDevicesOthers v-else-if="activeView === 'others'" @back="goBack" />
    <HaexSystemSettingsLayout v-else :title="t('title')" :description="t('description')">
      <div class="space-y-1">
        <HaexSystemSettingsLayoutMenuItem :label="t('menu.current')" :description="t('menu.currentDesc')" icon="i-lucide-monitor" @click="navigateTo('current')" />
        <HaexSystemSettingsLayoutMenuItem :label="t('menu.others')" :description="t('menu.othersDesc')" icon="i-lucide-smartphone" @click="navigateTo('others')" />
      </div>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
const { activeView, navigateTo, goBack } = useDrillDownNavigation<'index' | 'current' | 'others'>('index', 'devices')
</script>

<i18n lang="yaml">
de:
  title: Gerät
  description: Informationen über dieses Gerät
  menu:
    current: Aktuelles Gerät
    currentDesc: Geräteinformationen und Einstellungen
    others: Andere Geräte
    othersDesc: Andere verbundene Geräte
en:
  title: Device
  description: Information about this device
  menu:
    current: Current Device
    currentDesc: Device info and settings
    others: Other Devices
    othersDesc: Other connected devices
</i18n>
