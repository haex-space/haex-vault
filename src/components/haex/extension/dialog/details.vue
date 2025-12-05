<template>
  <UiDrawerModal
    v-model:open="open"
    :ui="{
      content: 'sm:max-w-6xl sm:mx-auto',
    }"
  >
    <template #header>
      <div class="flex items-center gap-4 w-full">
        <div
          v-if="extension?.iconUrl"
          class="w-12 h-12 rounded-lg overflow-hidden bg-primary/10 shrink-0"
        >
          <img
            :src="extension.iconUrl"
            :alt="extension.name"
            class="w-full h-full object-cover"
          >
        </div>
        <div
          v-else
          class="w-12 h-12 rounded-lg bg-gray-200 dark:bg-gray-700 flex items-center justify-center shrink-0"
        >
          <UIcon
            name="i-heroicons-puzzle-piece"
            class="w-8 h-8 text-gray-400"
          />
        </div>
        <div class="flex-1 min-w-0">
          <h3 class="text-lg font-semibold truncate">
            {{ extension?.name }}
          </h3>
          <div class="flex items-center gap-4 text-sm text-gray-500 dark:text-gray-400">
            <span v-if="extension?.publisher">
              {{ t('by') }} {{ extension.publisher.displayName }}
            </span>
            <div class="flex items-center gap-1">
              <UIcon name="i-heroicons-arrow-down-tray" />
              <span>{{ formatNumber(extension?.totalDownloads ?? 0) }}</span>
            </div>
            <div class="flex items-center gap-1">
              <UIcon
                name="i-heroicons-star-solid"
                class="text-yellow-500"
              />
              <span v-if="extension?.averageRating">{{ formatRating(extension.averageRating) }}</span>
              <span v-else>–</span>
            </div>
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <UButton
            :label="installButtonLabel"
            :color="isInstalled ? 'neutral' : 'primary'"
            :disabled="isInstalled && !hasUpdate"
            :icon="isInstalled && !hasUpdate ? 'i-heroicons-check' : 'i-heroicons-arrow-down-tray'"
            @click="onInstall"
          />
          <UButton
            icon="i-heroicons-x-mark"
            color="neutral"
            variant="ghost"
            @click="open = false"
          />
        </div>
      </div>
    </template>

    <template #content>
      <!-- Loading -->
      <div
        v-if="isLoading"
        class="flex justify-center py-8"
      >
        <UIcon
          name="i-heroicons-arrow-path"
          class="w-8 h-8 animate-spin text-gray-400"
        />
      </div>

      <div
        v-else-if="detail"
        class="flex flex-col gap-6"
      >
        <!-- Stats -->
        <div class="flex items-center gap-6 text-sm text-gray-500 dark:text-gray-400">
          <div
            v-if="detail.averageRating"
            class="flex items-center gap-1"
          >
            <UIcon
              name="i-heroicons-star-solid"
              class="text-yellow-500"
            />
            <span>{{ formatRating(detail.averageRating) }}</span>
            <span class="text-gray-400">({{ detail.reviewCount }})</span>
          </div>
          <UBadge
            v-if="detail.verified"
            color="success"
            variant="subtle"
          >
            <template #leading>
              <UIcon name="i-heroicons-check-badge-solid" />
            </template>
            {{ t('verified') }}
          </UBadge>
        </div>

        <!-- Description -->
        <MdPreview
          :model-value="detail.description || detail.shortDescription"
          :theme="isDark ? 'dark' : 'light'"
          preview-theme="default"
          class="markdown-preview"
        />

        <!-- Screenshots -->
        <div v-if="detail.screenshots?.length">
          <h4 class="font-semibold mb-2">
            {{ t('screenshots') }}
          </h4>
          <div class="flex gap-2 overflow-x-auto pb-2">
            <img
              v-for="screenshot in detail.screenshots"
              :key="screenshot.id"
              :src="screenshot.imageUrl"
              :alt="screenshot.caption || ''"
              class="h-40 rounded-lg object-cover"
            >
          </div>
        </div>

        <!-- Version Info -->
        <div v-if="detail.latestVersion">
          <h4 class="font-semibold mb-2">
            {{ t('latestVersion') }}
          </h4>
          <UCard>
            <div class="flex items-center justify-between">
              <div>
                <p class="font-medium">
                  v{{ detail.latestVersion.version }}
                </p>
                <p
                  v-if="detail.latestVersion.publishedAt"
                  class="text-sm text-gray-500"
                >
                  {{ formatDate(detail.latestVersion.publishedAt) }}
                </p>
              </div>
              <p
                v-if="detail.latestVersion.bundleSize"
                class="text-sm text-gray-500"
              >
                {{ formatSize(detail.latestVersion.bundleSize) }}
              </p>
            </div>
            <p
              v-if="detail.latestVersion.changelog"
              class="mt-2 text-sm text-gray-600 dark:text-gray-300"
            >
              {{ detail.latestVersion.changelog }}
            </p>
          </UCard>
        </div>

        <!-- Permissions -->
        <div v-if="detail.latestVersion?.permissions?.length">
          <h4 class="font-semibold mb-2">
            {{ t('permissions') }}
          </h4>
          <div class="flex flex-wrap gap-2">
            <UBadge
              v-for="permission in detail.latestVersion.permissions"
              :key="permission"
              color="neutral"
              variant="subtle"
            >
              {{ permission }}
            </UBadge>
          </div>
        </div>

        <!-- Tags -->
        <div v-if="detail.tags?.length">
          <h4 class="font-semibold mb-2">
            {{ t('tags') }}
          </h4>
          <div class="flex flex-wrap gap-2">
            <UBadge
              v-for="tag in detail.tags"
              :key="tag"
              color="primary"
              variant="soft"
            >
              {{ tag }}
            </UBadge>
          </div>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { MdPreview } from 'md-editor-v3'
import 'md-editor-v3/lib/preview.css'
import type { MarketplaceExtensionViewModel } from '~/types/haexspace'
import type { ExtensionDetail } from '@haex-space/marketplace-sdk'
import { useMarketplace } from '@haex-space/marketplace-sdk/vue'

const props = defineProps<{
  extension: MarketplaceExtensionViewModel | null
}>()

const emit = defineEmits<{
  install: [extension: MarketplaceExtensionViewModel]
}>()

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
const colorMode = useColorMode()
const marketplace = useMarketplace()

const isDark = computed(() => colorMode.value === 'dark')

const detail = ref<ExtensionDetail | null>(null)
const isLoading = ref(false)

const isInstalled = computed(() => props.extension?.isInstalled ?? false)
const hasUpdate = computed(() => {
  if (!props.extension?.installedVersion || !detail.value?.latestVersion) return false
  return props.extension.installedVersion !== detail.value.latestVersion.version
})

const installButtonLabel = computed(() => {
  if (!isInstalled.value) return t('install')
  if (hasUpdate.value) return t('update')
  return t('installed')
})

// Load details when modal opens
watch(open, async (isOpen) => {
  if (isOpen && props.extension) {
    isLoading.value = true
    try {
      detail.value = await marketplace.fetchExtension(props.extension.slug)
    } catch (error) {
      console.error('Failed to load extension details:', error)
    } finally {
      isLoading.value = false
    }
  } else {
    detail.value = null
  }
})

const onInstall = () => {
  if (props.extension) {
    emit('install', props.extension)
    open.value = false
  }
}

const formatNumber = (num: number) => {
  if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`
  if (num >= 1000) return `${(num / 1000).toFixed(1)}K`
  return num.toString()
}

const formatRating = (rating: number) => {
  return (rating / 100).toFixed(1)
}

const formatSize = (bytes: number) => {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}

const formatDate = (dateString: string) => {
  return new Date(dateString).toLocaleDateString()
}
</script>

<i18n lang="yaml">
de:
  by: von
  downloads: Downloads
  verified: Verifiziert
  description: Beschreibung
  screenshots: Screenshots
  latestVersion: Neueste Version
  permissions: Berechtigungen
  tags: Tags
  close: Schließen
  install: Installieren
  update: Aktualisieren
  installed: Installiert

en:
  by: by
  downloads: downloads
  verified: Verified
  description: Description
  screenshots: Screenshots
  latestVersion: Latest Version
  permissions: Permissions
  tags: Tags
  close: Close
  install: Install
  update: Update
  installed: Installed
</i18n>

<style>
.markdown-preview.md-editor {
  --md-theme-bg-color: transparent !important;
  background-color: transparent !important;
}

.markdown-preview .md-editor-preview-wrapper {
  background-color: transparent !important;
}

.markdown-preview .md-editor-preview {
  background-color: transparent !important;
  color: var(--ui-text-muted) !important;
}

.markdown-preview .md-editor-preview h1,
.markdown-preview .md-editor-preview h2,
.markdown-preview .md-editor-preview h3,
.markdown-preview .md-editor-preview h4,
.markdown-preview .md-editor-preview h5,
.markdown-preview .md-editor-preview h6 {
  color: var(--ui-text) !important;
  border-color: var(--ui-border) !important;
}

.markdown-preview .md-editor-preview p,
.markdown-preview .md-editor-preview li,
.markdown-preview .md-editor-preview td,
.markdown-preview .md-editor-preview th {
  color: var(--ui-text-muted) !important;
}

.markdown-preview .md-editor-preview a {
  color: var(--ui-primary) !important;
}

.markdown-preview .md-editor-preview code {
  background-color: var(--ui-bg-elevated) !important;
  color: var(--ui-text) !important;
}

.markdown-preview .md-editor-preview pre {
  background-color: var(--ui-bg-elevated) !important;
}

.markdown-preview .md-editor-preview blockquote {
  border-color: var(--ui-border) !important;
  color: var(--ui-text-muted) !important;
}

.markdown-preview .default-theme {
  background-color: transparent !important;
}
</style>
