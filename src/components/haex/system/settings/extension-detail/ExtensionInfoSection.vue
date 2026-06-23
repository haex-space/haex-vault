<template>
  <HaexSystemSettingsLayoutSection
    :title="t('info')"
    default-open
  >
    <template #actions>
      <UiButton
        v-if="hasUpdate && !extension.devServerUrl"
        :label="t('update')"
        icon="i-heroicons-arrow-up-circle"
        color="warning"
        :loading="isUpdating"
        @click="emit('update')"
      />
      <UiButton
        :label="t('remove')"
        icon="i-heroicons-trash"
        color="error"
        variant="outline"
        @click="emit('remove')"
      />
      <UiButton
        :label="t('open')"
        icon="i-heroicons-play"
        @click="emit('open')"
      />
    </template>

    <div class="space-y-3">
      <!-- Icon and Info Row -->
      <div class="flex items-start gap-3">
        <div
          class="w-16 h-16 shrink-0 rounded-lg bg-elevated flex items-center justify-center overflow-hidden"
        >
          <HaexIcon
            :name="extension.iconUrl || 'i-heroicons-puzzle-piece'"
            class="w-full h-full object-contain"
          />
        </div>

        <div class="flex-1 min-w-0 text-sm space-y-1">
          <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span class="font-medium">{{ t('version') }}:</span>
            <span>{{ extension.version }}</span>
            <!-- Loading indicator while checking for updates -->
            <UIcon
              v-if="isCheckingUpdate"
              name="i-heroicons-arrow-path"
              class="w-4 h-4 animate-spin text-gray-400"
            />
            <!-- Latest version badge -->
            <UBadge
              v-if="latestAvailableVersion && !isCheckingUpdate"
              :color="hasUpdate ? 'warning' : 'success'"
              variant="subtle"
              size="md"
            >
              {{ hasUpdate ? t('latestVersion', { version: latestAvailableVersion }) : t('upToDate') }}
            </UBadge>
          </div>
          <div v-if="extension.author">
            <span class="font-medium">{{ t('author') }}:</span>
            {{ extension.author }}
          </div>
        </div>
      </div>

      <div
        v-if="extension.description"
        class="text-sm text-gray-600 dark:text-gray-300"
      >
        {{ extension.description }}
      </div>
    </div>
  </HaexSystemSettingsLayoutSection>
</template>

<script setup lang="ts">
import type { IHaexSpaceExtension } from '~/types/haexspace'

defineProps<{
  extension: IHaexSpaceExtension
  hasUpdate: boolean
  isUpdating: boolean
  isCheckingUpdate: boolean
  latestAvailableVersion: string | null
}>()

const emit = defineEmits<{
  update: []
  remove: []
  open: []
}>()

const { t } = useI18n()
</script>
