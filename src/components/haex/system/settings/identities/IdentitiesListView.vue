<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
  >
    <template #actions>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-import"
        @click="emit('import-click')"
      >
        <span class="hidden @sm:inline">{{ t('actions.import') }}</span>
      </UButton>
      <UButton
        color="primary"
        icon="i-lucide-plus"
        data-tour="settings-identities-create"
        @click="emit('create-click')"
      >
        <span class="hidden @sm:inline">{{ t('actions.create') }}</span>
      </UButton>
    </template>

    <!-- Loading -->
    <div
      v-if="isLoading"
      class="flex items-center justify-center py-8"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-5 h-5 animate-spin text-primary"
      />
    </div>

    <!-- Identities list -->
    <div
      v-else-if="identities.length"
      class="space-y-3"
    >
      <IdentityListItem
        v-for="identity in identities"
        :key="identity.id"
        :identity="identity"
        :expanded="expandedIdentity === identity.id"
        :claims="claimsFor(identity.id)"
        :deletable="isDeletable(identity)"
        @toggle="(open) => emit('toggle', identity.id, open)"
        @share-qr="emit('share-qr', identity)"
        @copy-did="emit('copy-did', identity.did)"
        @export="emit('export', identity)"
        @edit="emit('edit', identity)"
        @delete="emit('delete', identity)"
        @add-claim="emit('add-claim', identity.id)"
        @copy-claim="(value) => emit('copy-claim', value)"
        @edit-claim="(claim) => emit('edit-claim', identity.id, claim)"
        @delete-claim="(claimId) => emit('delete-claim', claimId, identity.id)"
      />
    </div>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('list.empty')"
      icon="i-lucide-fingerprint"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'
import IdentityListItem, {
  type ListItemClaim,
} from './IdentityListItem.vue'

const { t } = useI18n()

const props = defineProps<{
  identities: SelectHaexIdentities[]
  isLoading: boolean
  expandedIdentity: string | null
  claimsFor: (identityId: string) => ListItemClaim[]
  isDeletable: (identity: SelectHaexIdentities) => boolean
}>()

const emit = defineEmits<{
  'import-click': []
  'create-click': []
  toggle: [identityId: string, open: boolean]
  'share-qr': [identity: SelectHaexIdentities]
  'copy-did': [did: string]
  export: [identity: SelectHaexIdentities]
  edit: [identity: SelectHaexIdentities]
  delete: [identity: SelectHaexIdentities]
  'add-claim': [identityId: string]
  'copy-claim': [value: string]
  'edit-claim': [identityId: string, claim: ListItemClaim]
  'delete-claim': [claimId: string, identityId: string]
}>()

// Silence unused-props warning (props are accessed via template).
void props
</script>
