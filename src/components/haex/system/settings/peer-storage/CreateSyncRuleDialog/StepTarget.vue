<template>
  <div class="space-y-4 pt-4">
    <USelectMenu
      v-model="type"
      :items="providerTypes"
      value-key="value"
      :label="t('target.type')"
      class="w-full"
    />

    <!-- Local folder picker -->
    <div v-if="type === 'local'" class="space-y-2">
      <UButton
        icon="i-lucide-folder"
        color="neutral"
        variant="outline"
        block
        @click="onSelectFolder"
      >
        {{ path || t('target.selectFolder') }}
      </UButton>
    </div>

    <!-- Peer: space + device + folder -->
    <div v-if="type === 'peer'" class="space-y-3">
      <UiSelectMenu
        v-model="spaceId"
        :items="spaceOptions"
        :label="t('target.space')"
        value-key="value"
      />
      <UiSelectMenu
        v-if="spaceId"
        v-model="deviceEndpointId"
        :items="deviceOptionsForSpace(spaceId)"
        :label="t('target.device')"
        value-key="value"
      />
      <template v-if="deviceEndpointId">
        <!-- Toggle: existing folder vs new folder -->
        <div class="flex items-center gap-2">
          <label class="text-sm font-medium flex-1">{{ t('target.folder') }}</label>
          <UButton
            size="xs"
            variant="link"
            :icon="createNewFolder ? 'i-lucide-list' : 'i-lucide-folder-plus'"
            @click="createNewFolder = !createNewFolder; shareId = ''; newFolderName = ''"
          >
            {{ createNewFolder ? t('target.chooseExisting') : t('target.createNew') }}
          </UButton>
        </div>
        <!-- Existing folder -->
        <UiSelectMenu
          v-if="!createNewFolder"
          v-model="shareId"
          :items="shareOptionsForDevice(deviceEndpointId)"
          value-key="value"
        />
        <!-- New folder name -->
        <UiInput
          v-else
          v-model="newFolderName"
          :placeholder="t('target.newFolderPlaceholder')"
        />
      </template>
      <UiInput
        v-if="shareId || newFolderName"
        v-model="subfolder"
        :label="t('target.subfolder')"
        :placeholder="t('target.subfolderPlaceholder')"
      />
    </div>

    <!-- Cloud: backend + bucket + prefix -->
    <div v-if="type === 'cloud'" class="space-y-3">
      <UiSelectMenu
        v-model="backendId"
        :items="backendOptions"
        :label="t('target.backend')"
        value-key="value"
      />
      <UiInput
        v-model="bucket"
        :label="t('target.bucket')"
        :placeholder="defaultBucketFor(backendId) || 'my-bucket'"
        :description="t('target.bucketDescription')"
      />
      <UiInput
        v-model="prefix"
        :label="t('target.prefix')"
        placeholder="backup/"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ProviderType } from '~/composables/peerStorage/useCreateSyncRuleWizard'

type SelectOption = { label: string; value: string; icon?: string }

defineProps<{
  providerTypes: { value: ProviderType; label: string; icon: string }[]
  spaceOptions: SelectOption[]
  backendOptions: SelectOption[]
  deviceOptionsForSpace: (spaceId: string) => SelectOption[]
  shareOptionsForDevice: (endpointId: string) => SelectOption[]
  defaultBucketFor: (backendId: string) => string
}>()

const emit = defineEmits<{
  selectFolder: []
}>()

const type = defineModel<ProviderType>('type', { required: true })
const path = defineModel<string>('path', { required: true })
const spaceId = defineModel<string>('spaceId', { required: true })
const deviceEndpointId = defineModel<string>('deviceEndpointId', { required: true })
const shareId = defineModel<string>('shareId', { required: true })
const createNewFolder = defineModel<boolean>('createNewFolder', { required: true })
const newFolderName = defineModel<string>('newFolderName', { required: true })
const subfolder = defineModel<string>('subfolder', { required: true })
const backendId = defineModel<string>('backendId', { required: true })
const bucket = defineModel<string>('bucket', { required: true })
const prefix = defineModel<string>('prefix', { required: true })

const { t } = useI18n()

const onSelectFolder = () => emit('selectFolder')
</script>
