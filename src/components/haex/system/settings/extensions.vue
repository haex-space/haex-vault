<template>
  <Transition :name="direction === 'back' ? 'slide-back' : 'slide-forward'" mode="out-in">
    <div :key="activeView" class="h-full">
      <HaexSystemSettingsExtensionsInstalled v-if="activeView === 'installed'" @back="goBack" />
      <HaexSystemSettingsExtensionsMarketplaces v-else-if="activeView === 'marketplaces'" @back="goBack" />
      <HaexSystemSettingsLayout v-else :title="t('title')" :description="t('description')">
        <div class="space-y-1">
          <HaexSystemSettingsLayoutMenuItem
            :label="t('menu.installed')"
            :description="t('menu.installedDesc')"
            icon="i-heroicons-puzzle-piece"
            @click="navigateTo('installed')"
          />
          <HaexSystemSettingsLayoutMenuItem
            :label="t('menu.marketplaces')"
            :description="t('menu.marketplacesDesc')"
            icon="i-mdi-store"
            @click="navigateTo('marketplaces')"
          />
        </div>
      </HaexSystemSettingsLayout>
    </div>
  </Transition>
</template>

<script setup lang="ts">
const { t } = useI18n()
const tabId = inject<string>('haex-tab-id')!
const { activeView, direction, navigateTo, goBack } = useDrillDownNavigation<'index' | 'installed' | 'marketplaces'>('index', 'extensions', tabId)
</script>

<i18n lang="yaml">
de:
  title: Erweiterungen
  description: Verwalte installierte Erweiterungen und konfiguriere Marketplaces.
  menu:
    installed: Erweiterungen anzeigen
    installedDesc: Installierte Erweiterungen verwalten und Berechtigungen prüfen
    marketplaces: Marketplace konfigurieren
    marketplacesDesc: Marketplaces hinzufügen, bearbeiten oder löschen
en:
  title: Extensions
  description: Manage installed extensions and configure marketplaces.
  menu:
    installed: Show extensions
    installedDesc: Manage installed extensions and review their permissions
    marketplaces: Configure marketplaces
    marketplacesDesc: Add, edit, or remove marketplaces
</i18n>
