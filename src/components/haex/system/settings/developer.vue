<template>
  <div class="h-full">
    <HaexSystemSettingsDeveloperAdd v-if="activeView === 'add'" @back="goBack" />
    <HaexSystemSettingsDeveloperList v-else-if="activeView === 'list'" @back="goBack" />
    <HaexSystemSettingsLayout v-else :title="t('title')" :description="t('description')">
      <div class="space-y-1">
        <HaexSystemSettingsLayoutMenuItem :label="t('menu.add')" :description="t('menu.addDesc')" icon="i-lucide-plus-circle" @click="navigateTo('add')" />
        <HaexSystemSettingsLayoutMenuItem :label="t('menu.list')" :description="t('menu.listDesc')" icon="i-lucide-puzzle" @click="navigateTo('list')" />
      </div>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
const { t } = useI18n()
const { activeView, navigateTo, goBack } = useDrillDownNavigation<'index' | 'add' | 'list'>('index', 'developer')
</script>

<i18n lang="yaml">
de:
  title: Entwickler
  description: Lade Extensions im Entwicklungsmodus für schnelleres Testen mit Hot-Reload.
  menu:
    add: Extension hinzufügen
    addDesc: Dev-Extension aus lokalem Pfad laden
    list: Dev Extensions
    listDesc: Geladene Dev-Extensions verwalten
en:
  title: Developer
  description: Load extensions in development mode for faster testing with hot-reload.
  menu:
    add: Add Extension
    addDesc: Load dev extension from local path
    list: Dev Extensions
    listDesc: Manage loaded dev extensions
</i18n>
