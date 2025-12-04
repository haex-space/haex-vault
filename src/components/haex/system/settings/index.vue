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
      <HaexSystemSettingsWorkspace v-if="activeCategory === 'workspace'" />
      <HaexSystemSettingsNotifications
        v-if="activeCategory === 'notifications'"
      />
      <HaexSystemSettingsExtensions v-if="activeCategory === 'extensions'" />
      <HaexSystemSettingsDatabase v-if="activeCategory === 'database'" />
      <HaexSystemSettingsSync v-if="activeCategory === 'sync'" />
      <HaexSystemSettingsDeveloper v-if="activeCategory === 'developer'" />
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
    value: 'workspace',
    label: t('categories.workspace'),
    icon: 'i-heroicons-squares-2x2',
    active: activeCategory.value === 'workspace',
    click: () => {
      activeCategory.value = 'workspace'
    },
  },
  {
    value: 'notifications',
    label: t('categories.notifications'),
    icon: 'i-heroicons-bell',
    active: activeCategory.value === 'notifications',
    click: () => {
      activeCategory.value = 'notifications'
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
    value: 'developer',
    label: t('categories.developer'),
    icon: 'i-heroicons-code-bracket',
    active: activeCategory.value === 'developer',
    click: () => {
      activeCategory.value = 'developer'
    },
  },
])
</script>

<i18n lang="yaml">
de:
  categories:
    general: Allgemein
    appearance: Erscheinungsbild
    workspace: Arbeitsbereich
    notifications: Benachrichtigungen
    extensions: Erweiterungen
    database: Datenbank
    sync: Synchronisation
    developer: Entwickler
en:
  categories:
    general: General
    appearance: Appearance
    workspace: Workspace
    notifications: Notifications
    extensions: Extensions
    database: Database
    sync: Sync
    developer: Developer
</i18n>
