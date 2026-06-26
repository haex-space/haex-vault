<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
  >
    <template #actions>
      <!-- Invite Policy -->
      <div class="w-52">
        <UiSelectMenu
          :model-value="policyOption"
          :items="policyOptions"
          :label="t('policy.label')"
          :search-input="false"
          @update:model-value="emit('policy-change', $event)"
        />
      </div>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-log-in"
        @click="showJoinDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.join') }}</span>
      </UButton>
      <UButton
        color="primary"
        icon="i-lucide-plus"
        data-testid="spaces-create-trigger"
        data-tour="settings-spaces-create"
        @click="showCreateDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.create') }}</span>
      </UButton>
    </template>

    <!-- Loading -->
    <div
      v-if="isLoadingSpaces"
      class="flex items-center justify-center py-8"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-5 h-5 animate-spin text-primary"
      />
    </div>

    <!-- Unified Space list -->
    <div
      v-else-if="spaceListEntries.length"
      class="space-y-3"
    >
      <SpaceListItem
        v-for="entry in spaceListEntries"
        :key="entry.kind === 'pending' ? `pending:${entry.invite.id}` : `active:${entry.space.id}`"
        :space="entry.space"
        :pending="entry.kind === 'pending'"
        :invite="entry.kind === 'pending' ? entry.invite : undefined"
        :show-tour-anchors="entry.kind === 'active' && entry.space.id === firstActiveSpaceId"
        @select="emit('select-space', $event)"
        @accept="
          emit(
            'accept-invite',
            entry.kind === 'pending' ? entry.invite : undefined,
          )
        "
        @decline="
          emit(
            'decline-invite',
            entry.kind === 'pending' ? entry.invite : undefined,
          )
        "
        @edit="emit('edit-space', $event)"
        @add-share="emit('add-share', $event)"
        @invite-contact="emit('invite-contact', $event)"
        @invite-link="emit('invite-link', $event)"
        @delete="emit('delete-space', $event)"
        @leave="emit('leave-space', $event)"
      />
    </div>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('list.empty')"
      icon="i-lucide-layout-grid"
    />

    <SpaceCreateDialog
      v-model:open="showCreateDialog"
      :origin-url-options="originUrlOptions"
      :owner-identity-options="ownerIdentityOptions"
      :default-owner-identity-id="defaultOwnerIdentityId"
      :submitting="isCreating"
      @submit="emit('create-space', $event)"
      @navigate-to-sync="emit('navigate-to-sync')"
    />

    <SpaceJoinDialog
      v-model:open="showJoinDialog"
      :initial-invite-link="inviteLink"
      :submitting="isJoining"
      @submit="emit('join-space', $event)"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import SpaceListItem from './SpaceListItem.vue'
import SpaceCreateDialog, {
  type CreateSpacePayload,
} from './SpaceCreateDialog.vue'
import SpaceJoinDialog from './SpaceJoinDialog.vue'
import type { SpaceWithType } from '@/stores/spaces/types'
import type { SelectHaexPendingInvites } from '~/database/schemas'
import type { InvitePolicyValue } from '@/composables/useSpaceInvites'

export type SpaceListEntry =
  | { kind: 'active'; space: SpaceWithType }
  | { kind: 'pending'; space: SpaceWithType; invite: SelectHaexPendingInvites }

interface PolicyOption {
  label: string
  value: InvitePolicyValue
}

interface OptionEntry {
  label: string
  value: string
}

defineProps<{
  isLoadingSpaces: boolean
  spaceListEntries: SpaceListEntry[]
  firstActiveSpaceId: string | null
  policyOption: PolicyOption | undefined
  policyOptions: PolicyOption[]
  originUrlOptions: OptionEntry[]
  ownerIdentityOptions: OptionEntry[]
  defaultOwnerIdentityId: string
  isCreating: boolean
  isJoining: boolean
  inviteLink?: string
}>()

const showCreateDialog = defineModel<boolean>('showCreateDialog', { required: true })
const showJoinDialog = defineModel<boolean>('showJoinDialog', { required: true })

const emit = defineEmits<{
  'policy-change': [option: PolicyOption]
  'create-space': [payload: CreateSpacePayload]
  'join-space': [payload: { inviteLink: string }]
  'navigate-to-sync': []
  'select-space': [space: SpaceWithType]
  'accept-invite': [invite: SelectHaexPendingInvites | undefined]
  'decline-invite': [invite: SelectHaexPendingInvites | undefined]
  'edit-space': [space: SpaceWithType]
  'add-share': [payload: { space: SpaceWithType, type: 'folder' | 'file' }]
  'invite-contact': [space: SpaceWithType]
  'invite-link': [space: SpaceWithType]
  'delete-space': [space: SpaceWithType]
  'leave-space': [space: SpaceWithType]
}>()

const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  title: Spaces
  description: Verwalte deine Spaces und Einladungen
  policy:
    label: Einladungsrichtlinie
  actions:
    join: Beitreten
    create: Erstellen
  list:
    empty: Keine Spaces vorhanden
en:
  title: Spaces
  description: Manage your spaces and invitations
  policy:
    label: Invite policy
  actions:
    join: Join
    create: Create
  list:
    empty: No spaces yet
</i18n>
