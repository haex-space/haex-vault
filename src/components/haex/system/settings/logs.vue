<template>
  <div class="h-full">
    <HaexSystemSettingsLogsViewer v-if="activeView === 'viewer'" @back="goBack" />
    <HaexSystemSettingsLogsRetention v-else-if="activeView === 'retention'" @back="goBack" />
    <HaexSystemSettingsLayout v-else :title="t('title')">
      <div class="space-y-1">
        <HaexSystemSettingsLayoutMenuItem :label="t('menu.viewer')" :description="t('menu.viewerDesc')" icon="i-lucide-scroll-text" @click="navigateTo('viewer')" />
        <HaexSystemSettingsLayoutMenuItem :label="t('menu.retention')" :description="t('menu.retentionDesc')" icon="i-lucide-settings" @click="navigateTo('retention')" />
      </div>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
const { activeView, navigateTo, goBack } = useDrillDownNavigation<'index' | 'viewer' | 'retention'>('index')
</script>

<i18n lang="yaml">
de:
  title: Logs
  menu:
    viewer: Logs anzeigen
    viewerDesc: System- und Erweiterungs-Logs
    retention: Einstellungen
    retentionDesc: Aufbewahrungszeiten für Logs
en:
  title: Logs
  menu:
    viewer: View Logs
    viewerDesc: System and extension logs
    retention: Settings
    retentionDesc: Log retention settings
</i18n>
