<template>
  <div class="h-full">
    <!-- Subview -->
    <HaexSystemSettingsPeerStorageConnection
      v-if="activeView === 'connection'"
      @back="goBack"
    />
    <HaexSystemSettingsPeerStorageRelay
      v-else-if="activeView === 'relay'"
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
      </div>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
const store = usePeerStorageStore()
const { activeView, navigateTo, goBack } = useDrillDownNavigation<'index' | 'connection' | 'relay'>('index')

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
  status:
    active: Active
    inactive: Inactive
</i18n>
