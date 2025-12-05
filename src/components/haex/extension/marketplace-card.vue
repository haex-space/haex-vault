<template>
  <UCard
    :ui="{
      root: 'hover:shadow-lg transition-shadow duration-200 cursor-pointer',
      body: 'flex flex-col gap-3',
    }"
    @click="$emit('click')"
  >
    <div class="flex items-start gap-4">
      <!-- Icon -->
      <div class="shrink-0">
        <div
          v-if="extension.iconUrl"
          class="w-20 h-20 rounded-lg bg-primary/10 flex items-center justify-center overflow-hidden"
        >
          <img
            :src="extension.iconUrl"
            :alt="extension.name"
            class="w-full h-full object-cover"
          />
        </div>
        <div
          v-else
          class="w-20 h-20 rounded-lg bg-gray-200 dark:bg-gray-700 flex items-center justify-center"
        >
          <UIcon
            name="i-heroicons-puzzle-piece"
            class="w-12 h-12 text-gray-400"
          />
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 min-w-0">
        <div class="flex items-start justify-between gap-2">
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold truncate">
              {{ extension.name }}
            </h3>
            <p
              v-if="extension.publisher"
              class="text-sm text-gray-500 dark:text-gray-400"
            >
              {{ t('by') }} {{ extension.publisher.displayName }}
            </p>
          </div>
          <!-- Version badges -->
          <div class="flex flex-col items-end gap-1">
            <UBadge
              v-if="extension.latestVersion"
              :label="`v${extension.latestVersion}`"
              color="neutral"
              variant="subtle"
              size="sm"
            />
            <UBadge
              v-if="extension.isInstalled && extension.installedVersion"
              :label="
                t('installedVersionShort', {
                  version: extension.installedVersion,
                })
              "
              :color="hasUpdate ? 'warning' : 'success'"
              variant="subtle"
              size="sm"
            />
          </div>
        </div>

        <p
          v-if="extension.shortDescription"
          class="hidden @lg:flex text-sm text-gray-600 dark:text-gray-300 mt-2 line-clamp-2"
        >
          {{ extension.shortDescription }}
        </p>

        <!-- Stats -->
        <div
          class="flex items-center gap-4 mt-3 text-sm text-gray-500 dark:text-gray-400"
        >
          <div class="flex items-center gap-1">
            <UIcon name="i-heroicons-arrow-down-tray" />
            <span>{{ formatNumber(extension.totalDownloads ?? 0) }} </span>
          </div>
          <div class="flex items-center gap-1">
            <UIcon
              name="i-heroicons-star-solid"
              class="text-yellow-500"
            />
            <span v-if="extension.averageRating">{{
              formatRating(extension.averageRating)
            }}</span>
            <span v-else>–</span>
          </div>
          <div
            v-if="extension.verified"
            class="flex items-center gap-1 text-green-600 dark:text-green-400"
          >
            <UIcon name="i-heroicons-check-badge-solid" />
            <span>{{ t('verified') }}</span>
          </div>
        </div>

        <!-- Tags -->
        <div
          v-if="extension.tags?.length"
          class="flex flex-wrap gap-1 mt-2"
        >
          <UBadge
            v-for="tag in extension.tags.slice(0, 3)"
            :key="tag"
            :label="tag"
            size="xs"
            color="primary"
            variant="soft"
          />
        </div>
      </div>
    </div>

    <!-- Actions -->
    <template #footer>
      <div class="flex items-center justify-between gap-2">
        <UButton
          :label="t('details')"
          color="neutral"
          variant="ghost"
          size="sm"
          @click.stop="$emit('details')"
        />
        <div class="flex items-center gap-2">
          <UButton
            v-if="extension.isInstalled"
            icon="i-heroicons-trash"
            color="error"
            variant="ghost"
            size="sm"
            @click.stop="$emit('remove')"
          />
          <!-- Update button (shown when update is available) -->
          <UButton
            v-if="hasUpdate"
            :label="t('update')"
            color="warning"
            icon="i-heroicons-arrow-path"
            size="sm"
            @click.stop="$emit('update')"
          />
          <!-- Install button (shown when not installed) -->
          <UButton
            v-else-if="!extension.isInstalled"
            :label="t('install')"
            color="primary"
            icon="i-heroicons-arrow-down-tray"
            size="sm"
            @click.stop="$emit('install')"
          />
          <!-- Installed indicator (shown when installed and no update) -->
          <UButton
            v-else
            :label="t('installed')"
            color="neutral"
            icon="i-heroicons-check"
            size="sm"
            disabled
          />
        </div>
      </div>
    </template>
  </UCard>
</template>

<script setup lang="ts">
import type { MarketplaceExtensionViewModel } from '~/types/haexspace'

const props = defineProps<{
  extension: MarketplaceExtensionViewModel
}>()

defineEmits(['click', 'install', 'update', 'details', 'remove'])

const { t } = useI18n()

const hasUpdate = computed(() => {
  return (
    props.extension.isInstalled &&
    props.extension.installedVersion &&
    props.extension.latestVersion &&
    props.extension.installedVersion !== props.extension.latestVersion
  )
})

const formatNumber = (num: number) => {
  if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`
  if (num >= 1000) return `${(num / 1000).toFixed(1)}K`
  return num.toString()
}

const formatRating = (rating: number) => {
  // Rating is stored as 0-500 (0.0-5.0 * 100)
  return (rating / 100).toFixed(1)
}
</script>

<i18n lang="yaml">
de:
  by: von
  install: Installieren
  installed: Installiert
  installedVersion: 'Installiert (v{version})'
  installedVersionShort: 'v{version}'
  update: Aktualisieren
  details: Details
  verified: Verifiziert
en:
  by: by
  install: Install
  installed: Installed
  installedVersion: 'Installed (v{version})'
  installedVersionShort: 'v{version}'
  update: Update
  details: Details
  verified: Verified
</i18n>
