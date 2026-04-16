<template>
  <Transition :name="direction === 'back' ? 'slide-back' : 'slide-forward'" mode="out-in">
    <div :key="activeView" class="h-full">
      <HaexSystemSettingsSyncBackends v-if="activeView === 'backends'" @back="goBack" />
      <HaexSystemSettingsSyncConfig v-else-if="activeView === 'config'" @back="goBack" />
      <HaexSystemSettingsPeerStorageRelay v-else-if="activeView === 'relay'" @back="goBack" />
      <HaexSystemSettingsPeerStorageSyncRules v-else-if="activeView === 'sync-rules'" @back="goBack" />
      <HaexSystemSettingsLayout v-else :title="t('title')" :description="t('description')">
        <div class="space-y-1">
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.backends')" :description="t('menu.backendsDesc')" icon="i-lucide-server" @click="navigateTo('backends')" />
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.relay')" :description="t('menu.relayDesc')" icon="i-mdi-lan-connect" @click="navigateTo('relay')" />
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.syncRules')" :description="t('menu.syncRulesDesc')" icon="i-lucide-refresh-cw" @click="navigateTo('sync-rules')" />
          <HaexSystemSettingsLayoutMenuItem :label="t('menu.config')" :description="t('menu.configDesc')" icon="i-lucide-settings" @click="navigateTo('config')" />
        </div>
      </HaexSystemSettingsLayout>
    </div>
  </Transition>
</template>

<script setup lang="ts">
const { t } = useI18n()
const tabId = inject<string>('haex-tab-id')!
const { activeView, direction, navigateTo, goBack } = useDrillDownNavigation<'index' | 'backends' | 'config' | 'relay' | 'sync-rules'>('index', 'sync', tabId)
</script>

<i18n lang="yaml">
de:
  title: Synchronisation
  description: Verwalte Sync-Backends, P2P-Verbindungen und Einstellungen
  menu:
    backends: Sync-Backends
    backendsDesc: Verbundene Server für die Synchronisation
    relay: Relay-Server
    relayDesc: Relay für P2P-Verbindungen durch NAT konfigurieren
    syncRules: Sync-Regeln
    syncRulesDesc: Dateien automatisch zwischen Geräten synchronisieren
    config: Konfiguration
    configDesc: Push-, Pull- und P2P-Einstellungen
en:
  title: Synchronization
  description: Manage sync backends, P2P connections and settings
  menu:
    backends: Sync Backends
    backendsDesc: Connected servers for synchronization
    relay: Relay Server
    relayDesc: Configure relay for P2P connections through NAT
    syncRules: Sync Rules
    syncRulesDesc: Automatically synchronize files between devices
    config: Configuration
    configDesc: Push, pull and P2P settings
</i18n>
