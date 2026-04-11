<template>
  <Transition :name="direction === 'back' ? 'slide-back' : 'slide-forward'" mode="out-in">
    <div :key="activeView" class="h-full">
      <!-- Subview -->
      <HaexSystemSettingsPeerStorageConnection
        v-if="activeView === 'connection'"
        @back="goBack"
      />
      <HaexSystemSettingsPeerStorageRelay
        v-else-if="activeView === 'relay'"
        @back="goBack"
      />
      <HaexSystemSettingsPeerStorageSyncRules
        v-else-if="activeView === 'sync-rules'"
        @back="goBack"
      />

      <!-- Index / Menu -->
      <HaexSystemSettingsLayout
        v-else
        :title="t('title')"
        :description="t('description')"
      >
        <div class="space-y-1">
          <HaexSystemSettingsLayoutMenuItem
            :label="t('menu.connection')"
            :description="t('menu.connectionDesc')"
            icon="i-lucide-wifi"
            @click="navigateTo('connection')"
          >
            <template #badge>
              <UBadge
                :color="store.running ? 'success' : 'neutral'"
                variant="subtle"
                size="sm"
              >
                {{ store.running ? t('status.active') : t('status.inactive') }}
              </UBadge>
            </template>
          </HaexSystemSettingsLayoutMenuItem>

          <HaexSystemSettingsLayoutMenuItem
            :label="t('menu.relay')"
            :description="t('menu.relayDesc')"
            icon="i-lucide-server"
            @click="navigateTo('relay')"
          />

          <HaexSystemSettingsLayoutMenuItem
            :label="t('menu.syncRules')"
            :description="t('menu.syncRulesDesc')"
            icon="i-lucide-refresh-cw"
            @click="navigateTo('sync-rules')"
          />
        </div>
      </HaexSystemSettingsLayout>
    </div>
  </Transition>
</template>

<script setup lang="ts">
const { t } = useI18n()
const store = usePeerStorageStore()
const tabId = inject<string>('haex-tab-id')!
const { activeView, direction, navigateTo, goBack } = useDrillDownNavigation<'index' | 'connection' | 'relay' | 'sync-rules'>('index', 'peer-storage', tabId)

onMounted(async () => {
  await store.refreshStatusAsync()
})
</script>

<i18n lang="yaml">
de:
  title: P2P Storage
  description: Teile lokale Ordner direkt mit anderen Peers über eine verschlüsselte P2P-Verbindung
  menu:
    connection: Verbindung
    connectionDesc: Endpoint starten und Ordner teilen
    relay: Relay-Server
    relayDesc: Relay für NAT-Traversal konfigurieren
    syncRules: Sync-Regeln
    syncRulesDesc: Dateien automatisch zwischen Geräten synchronisieren
  status:
    active: Aktiv
    inactive: Inaktiv
en:
  title: P2P Storage
  description: Share local folders directly with other peers over an encrypted P2P connection
  menu:
    connection: Connection
    connectionDesc: Start endpoint and share folders
    relay: Relay Server
    relayDesc: Configure relay for NAT traversal
    syncRules: Sync Rules
    syncRulesDesc: Automatically synchronize files between devices
  status:
    active: Active
    inactive: Inactive
</i18n>
