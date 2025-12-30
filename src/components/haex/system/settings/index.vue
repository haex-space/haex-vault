<template>
  <HaexSystem :is-dragging="isDragging">
    <template #sidebar>
      <nav class="flex flex-col gap-1">
        <button
          v-for="category in categories"
          :key="category.value"
          :class="[
            'flex items-center gap-3 p-2.5 text-sm font-medium rounded-md transition-colors',
            'justify-center @xl:justify-start',
            category.active
              ? 'bg-primary text-white'
              : 'text-highlighted hover:bg-muted',
          ]"
          :title="category.label"
          @click="category.click"
        >
          <UIcon
            :name="category.icon"
            class="w-5 h-5 shrink-0"
          />
          <span class="hidden @xl:block">{{ category.label }}</span>
        </button>
      </nav>
    </template>

    <div class="flex-1 overflow-y-auto">
      <HaexSystemSettingsGeneral v-if="activeCategory === 'general'" />
      <HaexSystemSettingsAppearance v-if="activeCategory === 'appearance'" />
      <HaexSystemSettingsExtensions v-if="activeCategory === 'extensions'" />
      <HaexSystemSettingsExternalClients v-if="activeCategory === 'externalClients'" />
      <HaexSystemSettingsDatabase v-if="activeCategory === 'database'" />
      <HaexSystemSettingsSync v-if="activeCategory === 'sync'" />
      <HaexSystemSettingsDevices v-if="activeCategory === 'devices'" />
      <HaexSystemSettingsDeveloper v-if="activeCategory === 'developer'" />
      <HaexSystemDebugLogs v-if="activeCategory === 'debugLogs'" />
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
defineProps<{
  isDragging?: boolean
}>()

const { t } = useI18n()

const activeCategory = ref('general')

const categories = computed(() => [
  {
    value: 'general',
    label: t('categories.general'),
    icon: 'i-heroicons-cog-6-tooth',
    active: activeCategory.value === 'general',
    click: () => {
      activeCategory.value = 'general'
    },
  },
  {
    value: 'appearance',
    label: t('categories.appearance'),
    icon: 'i-heroicons-paint-brush',
    active: activeCategory.value === 'appearance',
    click: () => {
      activeCategory.value = 'appearance'
    },
  },
  {
    value: 'extensions',
    label: t('categories.extensions'),
    icon: 'i-heroicons-puzzle-piece',
    active: activeCategory.value === 'extensions',
    click: () => {
      activeCategory.value = 'extensions'
    },
  },
  {
    value: 'externalClients',
    label: t('categories.externalClients'),
    icon: 'i-heroicons-globe-alt',
    active: activeCategory.value === 'externalClients',
    click: () => {
      activeCategory.value = 'externalClients'
    },
  },
  {
    value: 'database',
    label: t('categories.database'),
    icon: 'i-heroicons-circle-stack',
    active: activeCategory.value === 'database',
    click: () => {
      activeCategory.value = 'database'
    },
  },
  {
    value: 'sync',
    label: t('categories.sync'),
    icon: 'i-heroicons-arrow-path',
    active: activeCategory.value === 'sync',
    click: () => {
      activeCategory.value = 'sync'
    },
  },
  {
    value: 'devices',
    label: t('categories.devices'),
    icon: 'i-heroicons-device-phone-mobile',
    active: activeCategory.value === 'devices',
    click: () => {
      activeCategory.value = 'devices'
    },
  },
  {
    value: 'developer',
    label: t('categories.developer'),
    icon: 'i-hugeicons-developer',
    active: activeCategory.value === 'developer',
    click: () => {
      activeCategory.value = 'developer'
    },
  },
  {
    value: 'debugLogs',
    label: t('categories.debugLogs'),
    icon: 'i-heroicons-bug-ant',
    active: activeCategory.value === 'debugLogs',
    click: () => {
      activeCategory.value = 'debugLogs'
    },
  },
])
</script>

<i18n lang="yaml">
de:
  categories:
    general: Allgemein
    appearance: Erscheinungsbild
    extensions: Erweiterungen
    externalClients: Externe Clients
    database: Datenbank
    sync: Synchronisation
    devices: Geräte
    developer: Entwickler
    debugLogs: Debug Logs
en:
  categories:
    general: General
    appearance: Appearance
    extensions: Extensions
    externalClients: External Clients
    database: Database
    sync: Sync
    devices: Devices
    developer: Developer
    debugLogs: Debug Logs
</i18n>
