<template>
  <HaexSyncBackendItem :backend="backend">
    <template #actions>
      <div class="flex gap-2">
        <UButton
          :color="backend.enabled ? 'neutral' : 'primary'"
          icon="i-lucide-power"
          :title="
            backend.enabled ? t('actions.disable') : t('actions.enable')
          "
          @click="$emit('toggle')"
        >
          {{ backend.enabled ? t('actions.disable') : t('actions.enable') }}
        </UButton>
        <UButton
          color="error"
          variant="ghost"
          icon="i-lucide-trash-2"
          :title="t('actions.deleteBackend')"
          @click="$emit('deleteBackend')"
        />
      </div>
    </template>

    <!-- Server Vaults for this backend -->
    <template
      v-if="groupedVaults"
      #default
    >
      <!-- Loading state -->
      <div
        v-if="groupedVaults.isLoading"
        class="flex items-center justify-center py-4"
      >
        <UIcon
          name="i-lucide-loader-2"
          class="w-5 h-5 animate-spin text-primary"
        />
      </div>

      <!-- Error state -->
      <div
        v-else-if="groupedVaults.error"
        class="text-center text-error text-sm py-4"
      >
        {{ groupedVaults.error }}
      </div>

      <!-- No vaults -->
      <div
        v-else-if="groupedVaults.vaults.length === 0"
        class="space-y-4"
      >
        <p class="text-center text-muted text-sm py-4">
          {{ t('vaultOverview.noVaults') }}
        </p>

        <!-- Re-Upload option when current vault is missing on server -->
        <div
          v-if="groupedVaults.currentVaultMissingOnServer"
          class="space-y-3"
        >
          <UAlert
            color="warning"
            icon="i-lucide-alert-triangle"
            :title="t('reUpload.warning.title')"
            :description="t('reUpload.warning.description')"
          />
          <div class="flex justify-end">
            <UButton
              color="primary"
              icon="i-lucide-upload"
              :loading="isReUploading"
              :disabled="isReUploading"
              @click="$emit('reUpload')"
            >
              {{ t('reUpload.button') }}
            </UButton>
          </div>
        </div>
      </div>

      <!-- Vaults list -->
      <div
        v-else
        class="divide-y divide-default"
      >
        <div
          v-for="vault in groupedVaults.vaults"
          :key="vault.spaceId"
          class="flex flex-col gap-2 py-5 px-3"
          :class="
            vault.spaceId === currentVaultId
              ? 'bg-primary/10  rounded-lg  border border-primary/20'
              : ''
          "
        >
          <div
            class="flex flex-col @xs:flex-row @xs:items-center @xs:justify-between gap-2"
          >
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <p class="font-medium text-base truncate">
                  {{
                    vault.decryptedName ||
                    t('vaultOverview.encryptedName')
                  }}
                </p>
                <UBadge
                  v-if="vault.spaceId === currentVaultId"
                  color="primary"
                  variant="subtle"
                >
                  {{ t('vaultOverview.currentVault') }}
                </UBadge>
              </div>
              <p class="text-sm text-muted mt-1">
                {{ t('vaultOverview.createdAt') }}:
                {{ formatDate(vault.createdAt) }}
              </p>
            </div>
            <!-- Delete button -->
            <div class="@xs:shrink-0 w-full @xs:w-auto">
              <UButton
                color="error"
                variant="ghost"
                icon="i-lucide-trash-2"
                class="w-full @xs:w-auto justify-center"
                @click="$emit('deleteServerVault', vault)"
              />
            </div>
          </div>
        </div>
      </div>
    </template>
  </HaexSyncBackendItem>
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'
import type {
  GroupedServerVaults,
  ServerVault,
} from '@/composables/useBackendsActions'

defineProps<{
  backend: SelectHaexSyncBackends
  groupedVaults: GroupedServerVaults | undefined
  currentVaultId: string | null | undefined
  isReUploading: boolean
}>()

defineEmits<{
  toggle: []
  deleteBackend: []
  deleteServerVault: [vault: ServerVault]
  reUpload: []
}>()

const { t, locale } = useI18n()

const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString(locale.value)
}
</script>
