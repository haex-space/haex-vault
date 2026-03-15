<template>
  <HaexSystem
    :is-dragging="isDragging"
    disable-content-scroll
  >
    <template #sidebar>
      <nav class="flex flex-col gap-1">
        <button
          v-for="cat in categories"
          :key="cat.value"
          :class="[
            'flex items-center gap-3 p-2.5 text-sm font-medium rounded-md transition-colors',
            'justify-center @xl:justify-start',
            cat.active
              ? 'bg-primary text-white'
              : 'text-highlighted hover:bg-muted',
          ]"
          :title="cat.label"
          :data-testid="`settings-category-${cat.value}`"
          :data-tour="cat.tourId"
          @click="cat.click"
        >
          <UIcon
            :name="cat.icon"
            class="w-5 h-5 shrink-0"
          />
          <span class="hidden @xl:block">{{ cat.label }}</span>
        </button>
      </nav>
    </template>

    <div class="h-full">
      <HaexSystemSettingsGeneral v-if="activeCategory === 'general'" />
      <HaexSystemSettingsAppearance v-if="activeCategory === 'appearance'" />
      <HaexSystemSettingsExtensions v-if="activeCategory === 'extensions'" />
      <HaexSystemSettingsExternalClients v-if="activeCategory === 'externalClients'" />
      <HaexSystemSettingsDatabase v-if="activeCategory === 'database'" />
      <HaexSystemSettingsSync v-if="activeCategory === 'sync'" />
      <HaexSystemSettingsSpaces v-if="activeCategory === 'spaces'" :invite-link="props.inviteLink" />
      <HaexSystemSettingsIdentities v-if="activeCategory === 'identities'" />
      <HaexSystemSettingsContacts v-if="activeCategory === 'contacts'" />
      <HaexSystemSettingsStorage v-if="activeCategory === 'storage'" />
      <HaexSystemSettingsPeerStorage v-if="activeCategory === 'peerStorage'" />
      <HaexSystemSettingsDevices v-if="activeCategory === 'devices'" />
      <HaexSystemSettingsLogs v-if="activeCategory === 'logs'" />
      <HaexSystemSettingsDeveloper v-if="activeCategory === 'developer'" />
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
const props = defineProps<{
  isDragging?: boolean
  category?: string
  inviteLink?: string
}>()

const { t } = useI18n()

const activeCategory = ref(props.category || 'general')

watch(() => props.category, (newCategory) => {
  if (newCategory) activeCategory.value = newCategory
})

const categories = computed(() => [
  {
    value: 'general',
    label: t('categories.general'),
    icon: 'i-heroicons-cog-6-tooth',
    active: activeCategory.value === 'general',
    tourId: 'settings-nav-general',
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
    tourId: 'settings-nav-extensions',
    click: () => {
      activeCategory.value = 'extensions'
    },
  },
  {
    value: 'contacts',
    label: t('categories.contacts'),
    icon: 'i-lucide-contact',
    active: activeCategory.value === 'contacts',
    click: () => {
      activeCategory.value = 'contacts'
    },
  },
  {
    value: 'identities',
    label: t('categories.identities'),
    icon: 'i-lucide-fingerprint',
    active: activeCategory.value === 'identities',
    tourId: 'settings-nav-identities',
    click: () => {
      activeCategory.value = 'identities'
    },
  },
  {
    value: 'sync',
    label: t('categories.sync'),
    icon: 'i-lucide-server',
    active: activeCategory.value === 'sync',
    tourId: 'settings-nav-sync',
    click: () => {
      activeCategory.value = 'sync'
    },
  },
  {
    value: 'spaces',
    label: t('categories.spaces'),
    icon: 'i-heroicons-user-group',
    active: activeCategory.value === 'spaces',
    click: () => {
      activeCategory.value = 'spaces'
    },
  },
  {
    value: 'storage',
    label: t('categories.storage'),
    icon: 'i-heroicons-cloud',
    active: activeCategory.value === 'storage',
    click: () => {
      activeCategory.value = 'storage'
    },
  },
  {
    value: 'peerStorage',
    label: t('categories.peerStorage'),
    icon: 'i-mdi-lan-connect',
    active: activeCategory.value === 'peerStorage',
    click: () => {
      activeCategory.value = 'peerStorage'
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
    icon: 'i-mdi-safe-square-outline',
    active: activeCategory.value === 'database',
    click: () => {
      activeCategory.value = 'database'
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
    value: 'logs',
    label: t('categories.logs'),
    icon: 'i-heroicons-document-text',
    active: activeCategory.value === 'logs',
    click: () => {
      activeCategory.value = 'logs'
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
])
</script>

<i18n lang="yaml">
de:
  categories:
    general: Allgemein
    appearance: Erscheinungsbild
    extensions: Erweiterungen
    externalClients: Externe Clients
    database: Vault
    sync: Synchronisation
    spaces: Spaces
    identities: Identitäten
    contacts: Kontakte
    storage: Cloud Storage
    peerStorage: P2P Storage
    devices: Geräte
    logs: Logs
    developer: Entwickler
en:
  categories:
    general: General
    appearance: Appearance
    extensions: Extensions
    externalClients: External Clients
    database: Vault
    sync: Sync
    spaces: Spaces
    identities: Identities
    contacts: Contacts
    storage: Cloud Storage
    peerStorage: P2P Storage
    devices: Devices
    logs: Logs
    developer: Developer
</i18n>
