<template>
  <Transition :name="direction === 'back' ? 'slide-back' : 'slide-forward'" mode="out-in">
    <div :key="activeView" class="h-full">
      <HaexSystemSettingsDevicesCurrent v-if="activeView === 'current'" @back="goBack" />
      <HaexSystemSettingsDevicesOthers v-else-if="activeView === 'others'" @back="goBack" />
      <HaexSystemSettingsDevicesMatrix v-else-if="activeView === 'matrix'" @back="goBack" />
      <HaexSystemSettingsLayout v-else :title="t('title')" :description="t('description')">
        <div class="space-y-1">
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.current')" :description="t('menu.currentDesc')" icon="i-lucide-monitor" @click="navigateTo('current')" />
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.others')" :description="t('menu.othersDesc')" icon="i-lucide-smartphone" @click="navigateTo('others')" />
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.matrix')" :description="t('menu.matrixDesc')" icon="i-lucide-grid-3x3" @click="navigateTo('matrix')" />
        </div>
      </HaexSystemSettingsLayout>
    </div>
  </Transition>
</template>

<script setup lang="ts">
const { t } = useI18n()
const tabId = inject<string>('haex-tab-id')!
const { activeView, direction, navigateTo, goBack } = useDrillDownNavigation<'index' | 'current' | 'others' | 'matrix'>('index', 'devices', tabId)
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
    matrix: Geräte & Spaces
    matrixDesc: Welches deiner Geräte ist in welchem Space erreichbar
en:
  title: Device
  description: Information about this device
  menu:
    current: Current Device
    currentDesc: Device info and settings
    others: Other Devices
    othersDesc: Other connected devices
    matrix: Devices & Spaces
    matrixDesc: Which of your devices is reachable in which space
</i18n>
