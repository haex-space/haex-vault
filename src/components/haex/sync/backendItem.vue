<template>
  <UCard>
    <UCollapsible v-model:open="expanded">
      <div class="flex flex-col gap-3">
        <!-- Top row: Name + expand toggle + Actions -->
        <div class="flex flex-wrap items-start justify-between gap-3 cursor-pointer">
          <div
            class="flex items-center gap-2 min-w-0 text-left transition-opacity duration-200"
            :class="{ 'opacity-50': !backend.enabled }"
          >
            <UIcon
              name="i-lucide-chevron-right"
              class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
              :class="{ 'rotate-90': expanded }"
            />
            <div class="min-w-0">
              <p class="font-medium">{{ backend.name }}</p>
              <p class="text-sm text-gray-500 dark:text-gray-400 truncate">
                {{ getBackendHostByUrl(backend.homeServerUrl) }}
              </p>
            </div>
          </div>
          <!-- Actions slot -->
          <div v-if="$slots.actions" @click.stop>
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

      <!-- Collapsible content slot (e.g., for vaults list) -->
      <template v-if="$slots.default" #content>
        <div
          class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700 transition-opacity duration-200"
          :class="{ 'opacity-50': !backend.enabled }"
        >
          <slot />
        </div>
      </template>
    </UCollapsible>
  </UCard>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'

defineProps<{
  backend: SelectHaexSyncBackends
}>()

const expanded = ref(false)
const { getBackendHostByUrl } = useSyncBackendsStore()
</script>
