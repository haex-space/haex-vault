<template>
  <UCard>
    <div
      class="flex flex-col @sm:flex-row @sm:items-center justify-between gap-3"
    >
      <div class="flex-1 min-w-0">
        <p class="font-medium">{{ backend.name }}</p>
        <p class="text-sm text-gray-500 dark:text-gray-400 truncate">
          {{ backend.serverUrl }}
        </p>
        <div class="flex flex-wrap gap-2 mt-2">
          <UBadge
            :color="backend.enabled ? 'success' : 'neutral'"
            variant="subtle"
            size="xs"
          >
            {{
              backend.enabled
                ? t('status.enabled')
                : t('status.disabled')
            }}
          </UBadge>
          <UBadge
            v-if="syncState?.isConnected"
            color="info"
            variant="subtle"
            size="xs"
          >
            {{ t('status.connected') }}
          </UBadge>
          <UBadge
            v-else-if="syncState?.isSyncing"
            color="warning"
            variant="subtle"
            size="xs"
          >
            {{ t('status.syncing') }}
          </UBadge>
          <!-- Loading indicator -->
          <UIcon
            v-if="loading"
            name="i-lucide-loader-2"
            class="w-4 h-4 animate-spin"
          />
          <!-- Error badge -->
          <UBadge
            v-if="error"
            color="error"
            variant="subtle"
            size="xs"
          >
            {{ t('status.error') }}
          </UBadge>
          <!-- Count badge -->
          <UBadge
            v-if="count !== undefined && !loading && !error"
            color="neutral"
            variant="subtle"
            size="xs"
          >
            {{ count }}
          </UBadge>
        </div>
      </div>
      <div
        v-if="showToggle"
        class="shrink-0"
      >
        <UButton
          size="sm"
          :color="backend.enabled ? 'neutral' : 'primary'"
          class="w-full @sm:w-auto"
          @click="emit('toggle', backend.id)"
        >
          {{
            backend.enabled
              ? t('actions.disable')
              : t('actions.enable')
          }}
        </UButton>
      </div>
    </div>

    <!-- Optional content slot (e.g., for vaults list) -->
    <div
      v-if="$slots.default"
      class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700"
    >
      <slot />
    </div>
  </UCard>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'

defineProps<{
  backend: SelectHaexSyncBackends
  syncState?: {
    isConnected: boolean
    isSyncing: boolean
  } | null
  showToggle?: boolean
  loading?: boolean
  error?: string | null
  count?: number
}>()

const emit = defineEmits<{
  toggle: [backendId: string]
}>()

const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  status:
    enabled: Aktiviert
    disabled: Deaktiviert
    connected: Verbunden
    syncing: Synchronisiert
    error: Fehler
  actions:
    enable: Aktivieren
    disable: Deaktivieren
en:
  status:
    enabled: Enabled
    disabled: Disabled
    connected: Connected
    syncing: Syncing
    error: Error
  actions:
    enable: Enable
    disable: Disable
</i18n>
