<template>
  <div>
    <!-- Detail View -->
    <HaexSystemSettingsExtensionDetail
      v-if="selectedExtension"
      :extension="selectedExtension"
      @back="selectedExtension = null"
      @removed="handleExtensionRemoved"
    />

    <!-- List View -->
    <HaexSystemSettingsLayout
      v-else
      :title="t('title')"
      :description="t('description')"
    >
      <div class="flex justify-end">
        <UiButton
          :label="t('openMarketplace')"
          icon="i-mdi-store"
          @click="openMarketplaceAsync"
        />
      </div>

      <div v-if="loading" class="flex justify-center py-8">
        <UIcon
          name="i-heroicons-arrow-path"
          class="w-8 h-8 animate-spin text-primary"
        />
      </div>

      <div
        v-else-if="!allExtensions.length"
        class="text-center py-8 text-muted"
      >
        {{ t('noExtensions') }}
      </div>

      <div v-else class="space-y-2">
        <button
          v-for="ext in allExtensions"
          :key="ext.id"
          class="w-full p-4 rounded-lg border border-base-300 bg-base-100 hover:bg-base-200 transition-colors text-left"
          @click="selectedExtension = ext"
        >
          <div class="flex items-center gap-3">
            <div
              class="w-10 h-10 shrink-0 rounded-lg bg-base-200 flex items-center justify-center overflow-hidden"
            >
              <HaexIcon
                :name="ext.iconUrl || 'i-heroicons-puzzle-piece'"
                class="w-full h-full object-contain"
              />
            </div>

            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="font-semibold truncate">{{ ext.name }}</span>
                <UBadge
                  v-if="ext.devServerUrl"
                  color="warning"
                  variant="subtle"
                  size="xs"
                >
                  {{ t('devExtension') }}
                </UBadge>
              </div>
              <div class="text-sm text-muted">
                v{{ ext.version }}
                <span v-if="ext.author" class="ml-1"
                  >• {{ ext.author }}</span
                >
              </div>
            </div>

            <UIcon
              name="i-heroicons-chevron-right"
              class="w-5 h-5 text-gray-400 shrink-0"
            />
          </div>
        </button>
      </div>
    </HaexSystemSettingsLayout>
  </div>
</template>

<script setup lang="ts">
import type { IHaexSpaceExtension } from '~/types/haexspace'

const { t } = useI18n()
const { add } = useToast()

const extensionsStore = useExtensionsStore()
const windowManager = useWindowManagerStore()
const { availableExtensions } = storeToRefs(extensionsStore)
const { loadExtensionsAsync } = extensionsStore

const loading = ref(true)
const allExtensions = ref<IHaexSpaceExtension[]>([])
const selectedExtension = ref<IHaexSpaceExtension | null>(null)

const loadAllExtensionsAsync = async () => {
  loading.value = true
  try {
    await loadExtensionsAsync()
    allExtensions.value = availableExtensions.value
  } catch (error) {
    console.error('Error loading extensions:', error)
    add({ description: t('loadError'), color: 'error' })
  } finally {
    loading.value = false
  }
}

const handleExtensionRemoved = async () => {
  selectedExtension.value = null
  await loadAllExtensionsAsync()
}

const openMarketplaceAsync = async () => {
  await windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'marketplace',
  })
}

onMounted(async () => {
  await loadAllExtensionsAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Erweiterungen
  description: Verwalte installierte Erweiterungen und deren Berechtigungen.
  openMarketplace: Marketplace öffnen
  noExtensions: Keine Erweiterungen installiert.
  devExtension: Dev
  loadError: Fehler beim Laden der Erweiterungen
en:
  title: Extensions
  description: Manage installed extensions and their permissions.
  openMarketplace: Open Marketplace
  noExtensions: No extensions installed.
  devExtension: Dev
  loadError: Error loading extensions
</i18n>
