<template>
  <Transition :name="direction === 'back' ? 'slide-back' : 'slide-forward'" mode="out-in">
    <div :key="activeView" class="h-full">
      <HaexSystemSettingsSyncBackends v-if="activeView === 'backends'" @back="goBack" />
      <HaexSystemSettingsSyncConfig v-else-if="activeView === 'config'" @back="goBack" />
      <HaexSystemSettingsLayout v-else :title="t('title')" :description="t('description')">
        <div class="space-y-1">
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.backends')" :description="t('menu.backendsDesc')" icon="i-lucide-server" @click="navigateTo('backends')" />
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.config')" :description="t('menu.configDesc')" icon="i-lucide-settings" @click="navigateTo('config')" />
        </div>
      </HaexSystemSettingsLayout>
    </div>
  </Transition>
</template>

<script setup lang="ts">
const { t } = useI18n()
const tabId = inject<string>('haex-tab-id')!
const { activeView, direction, navigateTo, goBack } = useDrillDownNavigation<'index' | 'backends' | 'config'>('index', 'sync', tabId)
</script>

<i18n lang="yaml">
de:
  title: Synchronisation
  description: Verwalte deine Sync-Backends und Account-Einstellungen
  menu:
    backends: Sync-Backends
    backendsDesc: Verbundene Server für die Synchronisation
    config: Konfiguration
    configDesc: Push- und Pull-Einstellungen
en:
  title: Synchronization
  description: Manage your sync backends and account settings
  menu:
    backends: Sync Backends
    backendsDesc: Connected servers for synchronization
    config: Configuration
    configDesc: Push and pull settings
</i18n>
