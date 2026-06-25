<template>
  <HaexSystemSettingsLayoutSection
    :title="t('permissions')"
  >
    <template #actions>
      <UiButton
        v-if="hasAnyPermissions"
        :label="t('savePermissions')"
        :loading="savingPermissions"
        :disabled="!hasPermissionChanges"
        @click="emit('save')"
      />
    </template>

    <div
      v-if="loadingPermissions"
      class="flex justify-center py-4"
    >
      <UIcon
        name="i-heroicons-arrow-path"
        class="w-6 h-6 animate-spin text-primary"
      />
    </div>

    <div
      v-else
      class="space-y-4"
    >
      <UAccordion
        v-if="hasAnyPermissions"
        :items="permissionAccordionItems"
        :ui="{ root: 'flex flex-col gap-2' }"
      >
        <template #database>
          <HaexExtensionPermissionList
            v-model="editablePermissions.database"
          />
        </template>
        <template #filesystem>
          <HaexExtensionPermissionList
            v-model="editablePermissions.filesystem"
          />
        </template>
        <template #http>
          <HaexExtensionPermissionList v-model="editablePermissions.http" />
        </template>
        <template #shell>
          <HaexExtensionPermissionList
            v-model="editablePermissions.shell"
          />
        </template>
        <template #syncServers>
          <HaexExtensionPermissionList
            v-model="editablePermissions.syncServers"
          />
        </template>
        <template #cloudStorage>
          <HaexExtensionPermissionList
            v-model="editablePermissions.cloudStorage"
          />
        </template>
        <template #syncRules>
          <HaexExtensionPermissionList
            v-model="editablePermissions.syncRules"
          />
        </template>
      </UAccordion>

      <HaexSystemSettingsLayoutEmpty
        v-if="!hasAnyPermissions"
        :message="t('noPermissions')"
        icon="i-heroicons-shield-check"
      />
    </div>
  </HaexSystemSettingsLayoutSection>
</template>

<script setup lang="ts">
import type { ExtensionPermissionsEditable } from '~/composables/useExtensionDetailState'

defineProps<{
  loadingPermissions: boolean
  savingPermissions: boolean
  hasAnyPermissions: boolean
  hasPermissionChanges: boolean
  permissionAccordionItems: any[]
}>()

const editablePermissions = defineModel<ExtensionPermissionsEditable>('editablePermissions', { required: true })

const emit = defineEmits<{
  save: []
}>()

const { t } = useI18n()
</script>
