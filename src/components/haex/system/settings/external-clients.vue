<template>
  <div class="h-full">
    <!-- Subviews -->
    <HaexSystemSettingsExternalClientsBridge
      v-if="activeView === 'bridge'"
      @back="goBack"
    />
    <HaexSystemSettingsExternalClientsManage
      v-else-if="activeView === 'clients'"
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
          :label="t('menu.bridge')"
          :description="t('menu.bridgeDesc')"
          icon="i-lucide-radio-tower"
          @click="navigateTo('bridge')"
        />
        <HaexSystemSettingsLayoutMenuItem
          :label="t('menu.clients')"
          :description="t('menu.clientsDesc')"
          icon="i-lucide-shield-check"
          @click="navigateTo('clients')"
        />
      </div>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
const { activeView, navigateTo, goBack } = useDrillDownNavigation<'index' | 'bridge' | 'clients'>('index')
</script>

<i18n lang="yaml">
de:
  title: Externe Clients
  description: Verwalte Browser-Erweiterungen, CLI-Tools und andere externe Anwendungen, die auf deine Vault zugreifen können.
  menu:
    bridge: Bridge-Konfiguration
    bridgeDesc: Port und Status der WebSocket-Bridge verwalten
    clients: Client-Verwaltung
    clientsDesc: Autorisierte, temporäre und blockierte Clients verwalten
en:
  title: External Clients
  description: Manage browser extensions, CLI tools, and other external applications that can access your vault.
  menu:
    bridge: Bridge Configuration
    bridgeDesc: Manage port and status of the WebSocket bridge
    clients: Client Management
    clientsDesc: Manage authorized, temporary, and blocked clients
</i18n>
