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
    <template v-else>
      <div class="p-6 border-b border-base-content/10">
        <h2 class="text-2xl font-bold">
          {{ t('title') }}
        </h2>
        <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
          {{ t('description') }}
        </p>
      </div>

      <div class="p-6 space-y-4">
        <div v-if="loading" class="flex justify-center py-8">
          <UIcon
            name="i-heroicons-arrow-path"
            class="w-8 h-8 animate-spin text-primary"
          />
        </div>

        <div
          v-else-if="!allExtensions.length"
          class="text-center py-8 text-gray-500 dark:text-gray-400"
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
                v-if="ext.icon"
                class="w-10 h-10 shrink-0 rounded-lg bg-base-200 flex items-center justify-center overflow-hidden"
              >
                <img
                  :src="ext.icon"
                  :alt="ext.name"
                  class="w-full h-full object-contain"
                />
              </div>
              <div
                v-else
                class="w-10 h-10 shrink-0 rounded-lg bg-base-200 flex items-center justify-center"
              >
                <UIcon name="i-heroicons-puzzle-piece" class="w-5 h-5" />
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
                <div class="text-sm text-gray-500 dark:text-gray-400">
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
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import type { ExtensionInfoResponse } from '~~/src-tauri/bindings/ExtensionInfoResponse'

const { t } = useI18n()
const { add } = useToast()

const extensionsStore = useExtensionsStore()
const { availableExtensions } = storeToRefs(extensionsStore)
const { loadExtensionsAsync } = extensionsStore

const loading = ref(true)
const allExtensions = ref<ExtensionInfoResponse[]>([])
const selectedExtension = ref<ExtensionInfoResponse | null>(null)

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

onMounted(async () => {
  await loadAllExtensionsAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Erweiterungen
  description: Verwalte installierte Erweiterungen und deren Berechtigungen.
  noExtensions: Keine Erweiterungen installiert.
  devExtension: Dev
  loadError: Fehler beim Laden der Erweiterungen
en:
  title: Extensions
  description: Manage installed extensions and their permissions.
  noExtensions: No extensions installed.
  devExtension: Dev
  loadError: Error loading extensions
</i18n>
