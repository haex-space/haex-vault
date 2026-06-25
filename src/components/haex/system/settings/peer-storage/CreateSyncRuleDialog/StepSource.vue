<template>
  <div class="space-y-4 pt-4">
    <USelectMenu
      v-model="type"
      :items="providerTypes"
      value-key="value"
      :label="t('source.type')"
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
        {{ path || t('source.selectFolder') }}
      </UButton>
    </div>

    <!-- Peer: space + device + share pickers -->
    <div v-if="type === 'peer'" class="space-y-3">
      <UiSelectMenu
        v-model="spaceId"
        :items="spaceOptions"
        :label="t('source.space')"
        value-key="value"
      />
      <UiSelectMenu
        v-if="spaceId"
        v-model="deviceEndpointId"
        :items="deviceOptionsForSpace(spaceId)"
        :label="t('source.device')"
        value-key="value"
      />
      <UiSelectMenu
        v-if="deviceEndpointId"
        v-model="shareId"
        :items="shareOptionsForDevice(deviceEndpointId)"
        :label="t('source.share')"
        value-key="value"
      />
      <UiInput
        v-if="shareId"
        v-model="subfolder"
        :label="t('source.subfolder')"
        :placeholder="t('source.subfolderPlaceholder')"
      />
    </div>

    <!-- Cloud: backend + bucket + prefix -->
    <div v-if="type === 'cloud'" class="space-y-3">
      <UiSelectMenu
        v-model="backendId"
        :items="backendOptions"
        :label="t('source.backend')"
        value-key="value"
      />
      <UiInput
        v-model="bucket"
        :label="t('source.bucket')"
        :placeholder="defaultBucketFor(backendId) || 'my-bucket'"
        :description="t('source.bucketDescription')"
      />
      <UiInput
        v-model="prefix"
        :label="t('source.prefix')"
        placeholder="photos/"
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
const subfolder = defineModel<string>('subfolder', { required: true })
const backendId = defineModel<string>('backendId', { required: true })
const bucket = defineModel<string>('bucket', { required: true })
const prefix = defineModel<string>('prefix', { required: true })

const { t } = useI18n()

const onSelectFolder = () => emit('selectFolder')
</script>
