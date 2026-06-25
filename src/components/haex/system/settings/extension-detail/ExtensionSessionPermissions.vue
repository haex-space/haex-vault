<template>
  <HaexSystemSettingsLayoutSection
    :title="t('sessionPermissions')"
    :description="t('sessionPermissionsDescription')"
  >
    <UiListContainer>
      <UiListItem
        v-for="permission in sessionPermissions"
        :key="`${permission.resourceType}-${permission.target}`"
      >
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <UIcon
              :name="getPermissionIcon(permission.resourceType)"
              class="w-4 h-4"
            />
            <span class="font-medium">{{ t(`permissionTypes.${getPermissionTypeKey(permission.resourceType)}`) }}</span>
            <UBadge
              :color="permission.status === 'granted' ? 'success' : 'error'"
              variant="subtle"
            >
              {{ permission.status === 'granted' ? t('sessionGranted') : t('sessionDenied') }}
            </UBadge>
          </div>
          <div class="text-sm text-gray-500 dark:text-gray-400 mt-1 font-mono truncate">
            {{ permission.target }}
          </div>
          <div class="text-xs text-gray-400 dark:text-gray-500 mt-1">
            {{ t('sessionHint') }}
          </div>
        </div>
        <template #actions>
          <UButton
            color="error"
            variant="ghost"
            :loading="revokingKey === `${permission.resourceType}-${permission.target}`"
            @click="emit('revoke', permission)"
          >
            <UIcon name="i-heroicons-x-mark" class="w-4 h-4" />
            {{ t('revoke') }}
          </UButton>
        </template>
      </UiListItem>
    </UiListContainer>
  </HaexSystemSettingsLayoutSection>
</template>

<script setup lang="ts">
import type { ExtensionPermission } from '~~/src-tauri/bindings/ExtensionPermission'

defineProps<{
  sessionPermissions: ExtensionPermission[]
  revokingKey: string | null
}>()

const emit = defineEmits<{
  revoke: [permission: ExtensionPermission]
}>()

const { t } = useI18n()

const getPermissionIcon = (resourceType: string): string => {
  const icons: Record<string, string> = {
    db: 'i-heroicons-circle-stack',
    fs: 'i-heroicons-folder',
    web: 'i-heroicons-globe-alt',
    shell: 'i-heroicons-command-line',
    syncServers: 'i-heroicons-server',
    cloudStorage: 'i-heroicons-cloud-arrow-up',
    syncRules: 'i-heroicons-arrow-path',
  }
  return icons[resourceType] || 'i-heroicons-shield-check'
}

const getPermissionTypeKey = (resourceType: string): string => {
  const keys: Record<string, string> = {
    db: 'database',
    fs: 'filesystem',
    web: 'http',
    shell: 'shell',
    syncServers: 'syncServers',
    cloudStorage: 'cloudStorage',
    syncRules: 'syncRules',
  }
  return keys[resourceType] || resourceType
}
</script>
