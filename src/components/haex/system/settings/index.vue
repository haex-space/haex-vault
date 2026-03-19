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
            'flex items-center gap-3 p-2 @3xl:p-3 text-base font-medium rounded-md transition-colors',
            'justify-center aspect-square @3xl:aspect-auto @3xl:justify-start',
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
            class="size-8 @3xl:size-6 shrink-0"
          />
          <span class="hidden @3xl:block">{{ cat.label }}</span>
        </button>
      </nav>
    </template>

    <div class="h-full">
      <HaexSystemSettingsGeneral v-if="activeCategory === SettingsCategory.General" />
      <HaexSystemSettingsAppearance v-if="activeCategory === SettingsCategory.Appearance" />
      <HaexSystemSettingsExtensions v-if="activeCategory === SettingsCategory.Extensions" />
      <HaexSystemSettingsExternalClients
        v-if="activeCategory === SettingsCategory.ExternalClients"
      />
      <HaexSystemSettingsDatabase v-if="activeCategory === SettingsCategory.Database" />
      <HaexSystemSettingsSync v-if="activeCategory === SettingsCategory.Sync" />
      <HaexSystemSettingsSpaces
        v-if="activeCategory === SettingsCategory.Spaces"
        :invite-link="props.inviteLink"
      />
      <HaexSystemSettingsIdentities v-if="activeCategory === SettingsCategory.Identities" />
      <HaexSystemSettingsContacts v-if="activeCategory === SettingsCategory.Contacts" />
      <HaexSystemSettingsStorage v-if="activeCategory === SettingsCategory.Storage" />
      <HaexSystemSettingsPeerStorage v-if="activeCategory === SettingsCategory.PeerStorage" />
      <HaexSystemSettingsDevices v-if="activeCategory === SettingsCategory.Devices" />
      <HaexSystemSettingsLogs v-if="activeCategory === SettingsCategory.Logs" />
      <HaexSystemSettingsDeveloper v-if="activeCategory === SettingsCategory.Developer" />
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
import { SettingsCategory, SettingsCategoryIcon } from '~/config/settingsCategories'

const props = defineProps<{
  isDragging?: boolean
  category?: string
  inviteLink?: string
}>()

const { t } = useI18n()

const activeCategory = ref(props.category || SettingsCategory.General)
const { pushBack } = useBackNavigation()

const navigateToCategory = (category: string) => {
  if (category === activeCategory.value) return

  const previous = activeCategory.value
  activeCategory.value = category

  pushBack({
    undo: () => {
      activeCategory.value = previous
    },
  })
}

watch(
  () => props.category,
  (newCategory) => {
    if (newCategory && newCategory !== activeCategory.value) {
      navigateToCategory(newCategory)
    }
  },
)

const categories = computed(() => [
  {
    value: SettingsCategory.General,
    label: t('categories.general'),
    icon: 'i-heroicons-cog-6-tooth',
    active: activeCategory.value === 'general',
    tourId: 'settings-nav-general',
    click: () => {
      navigateToCategory(SettingsCategory.General)
    },
  },
  {
    value: SettingsCategory.Appearance,
    label: t('categories.appearance'),
    icon: 'i-heroicons-paint-brush',
    active: activeCategory.value === 'appearance',
    click: () => {
      navigateToCategory(SettingsCategory.Appearance)
    },
  },
  {
    value: SettingsCategory.Extensions,
    label: t('categories.extensions'),
    icon: SettingsCategoryIcon[SettingsCategory.Extensions],
    active: activeCategory.value === 'extensions',
    tourId: 'settings-nav-extensions',
    click: () => {
      navigateToCategory(SettingsCategory.Extensions)
    },
  },
  {
    value: SettingsCategory.Contacts,
    label: t('categories.contacts'),
    icon: SettingsCategoryIcon[SettingsCategory.Contacts],
    active: activeCategory.value === 'contacts',
    click: () => {
      navigateToCategory(SettingsCategory.Contacts)
    },
  },
  {
    value: SettingsCategory.Identities,
    label: t('categories.identities'),
    icon: SettingsCategoryIcon[SettingsCategory.Identities],
    active: activeCategory.value === 'identities',
    tourId: 'settings-nav-identities',
    click: () => {
      navigateToCategory(SettingsCategory.Identities)
    },
  },
  {
    value: SettingsCategory.Sync,
    label: t('categories.sync'),
    icon: SettingsCategoryIcon[SettingsCategory.Sync],
    active: activeCategory.value === 'sync',
    tourId: 'settings-nav-sync',
    click: () => {
      navigateToCategory(SettingsCategory.Sync)
    },
  },
  {
    value: SettingsCategory.Spaces,
    label: t('categories.spaces'),
    icon: SettingsCategoryIcon[SettingsCategory.Spaces],
    active: activeCategory.value === 'spaces',
    click: () => {
      navigateToCategory(SettingsCategory.Spaces)
    },
  },
  {
    value: SettingsCategory.Storage,
    label: t('categories.storage'),
    icon: SettingsCategoryIcon[SettingsCategory.Storage],
    active: activeCategory.value === 'storage',
    click: () => {
      navigateToCategory(SettingsCategory.Storage)
    },
  },
  {
    value: SettingsCategory.PeerStorage,
    label: t('categories.peerStorage'),
    icon: SettingsCategoryIcon[SettingsCategory.PeerStorage],
    active: activeCategory.value === 'peerStorage',
    click: () => {
      navigateToCategory(SettingsCategory.PeerStorage)
    },
  },
  {
    value: SettingsCategory.ExternalClients,
    label: t('categories.externalClients'),
    icon: SettingsCategoryIcon[SettingsCategory.ExternalClients],
    active: activeCategory.value === 'externalClients',
    click: () => {
      navigateToCategory(SettingsCategory.ExternalClients)
    },
  },
  {
    value: SettingsCategory.Database,
    label: t('categories.database'),
    icon: SettingsCategoryIcon[SettingsCategory.Database],
    active: activeCategory.value === 'database',
    click: () => {
      navigateToCategory(SettingsCategory.Database)
    },
  },
  {
    value: SettingsCategory.Devices,
    label: t('categories.devices'),
    icon: SettingsCategoryIcon[SettingsCategory.Devices],
    active: activeCategory.value === 'devices',
    click: () => {
      navigateToCategory(SettingsCategory.Devices)
    },
  },
  {
    value: SettingsCategory.Logs,
    label: t('categories.logs'),
    icon: SettingsCategoryIcon[SettingsCategory.Logs],
    active: activeCategory.value === 'logs',
    click: () => {
      navigateToCategory(SettingsCategory.Logs)
    },
  },
  {
    value: SettingsCategory.Developer,
    label: t('categories.developer'),
    icon: SettingsCategoryIcon[SettingsCategory.Developer],
    active: activeCategory.value === 'developer',
    click: () => {
      navigateToCategory(SettingsCategory.Developer)
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
