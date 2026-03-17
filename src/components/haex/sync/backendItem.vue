<template>
  <UCard>
    <div class="flex flex-col gap-3">
      <!-- Top row: Name + Actions -->
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div class="min-w-0 transition-opacity duration-200" :class="{ 'opacity-50': !backend.enabled }">
          <p class="font-medium">{{ backend.name }}</p>
          <p class="text-sm text-gray-500 dark:text-gray-400 truncate">
            {{ getBackendHostByUrl(backend.serverUrl) }}
          </p>
        </div>
        <!-- Actions slot -->
        <div v-if="$slots.actions">
          <slot name="actions" />
        </div>
      </div>

      <!-- Badges row -->
      <div
        v-if="$slots.badges"
        class="flex flex-wrap items-center gap-2"
      >
        <slot name="badges" />
      </div>
    </div>

    <!-- Optional content slot (e.g., for vaults list) -->
    <div
      v-if="$slots.default"
      class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700 transition-opacity duration-200"
      :class="{ 'opacity-50': !backend.enabled }"
    >
      <slot />
    </div>
  </UCard>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'

defineProps<{
  backend: SelectHaexSyncBackends
}>()

const { getBackendHostByUrl } = useSyncBackendsStore()
</script>
