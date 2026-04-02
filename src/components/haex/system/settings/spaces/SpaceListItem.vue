<template>
  <div
    class="flex flex-col gap-2 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50 transition-colors"
    :class="[
      pending ? 'border border-dashed border-primary/30' : 'cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-700/50',
    ]"
    @click="!pending && $emit('select', space)"
  >
    <div
      class="flex flex-col @xs:flex-row @xs:items-center @xs:justify-between gap-2"
    >
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2 flex-wrap">
          <p class="font-medium text-sm truncate">
            {{ space.name }}
          </p>
          <UBadge
            v-if="pending"
            color="warning"
            variant="subtle"
            size="sm"
            icon="i-lucide-clock"
          >
            {{ t('status.pending') }}
          </UBadge>
          <UBadge
            v-if="!pending && space.serverUrl"
            color="info"
            variant="subtle"
            size="sm"
            icon="i-lucide-cloud"
          >
            {{ backendName }}
          </UBadge>
          <UBadge
            v-if="!pending && !space.serverUrl"
            color="neutral"
            variant="subtle"
            size="sm"
            icon="i-lucide-hard-drive"
          >
            {{ t('type.local') }}
          </UBadge>
          <UBadge
            v-if="!pending"
            :color="permissionBadgeColor"
            variant="subtle"
            size="sm"
          >
            {{ permissionLabel }}
          </UBadge>
        </div>
        <!-- Pending: inviter info + capabilities -->
        <template v-if="pending && invite">
          <p class="text-xs text-muted mt-1">
            {{ t('invite.from') }}: {{ invite.inviterLabel || invite.inviterDid }}
          </p>
          <p v-if="invite.capabilities" class="text-xs text-muted">
            {{ formatCapabilities(invite.capabilities) }}
          </p>
        </template>
        <!-- Active: created date -->
        <p v-else class="text-xs text-gray-500 dark:text-gray-400 mt-1">
          {{ t('createdAt') }}: {{ formatDate(space.createdAt) }}
        </p>
      </div>
      <div class="flex gap-2 @xs:shrink-0">
        <!-- Pending: Accept/Decline only -->
        <template v-if="pending">
          <UButton
            color="primary"
            variant="soft"
            icon="i-lucide-check"
            @click.stop="$emit('accept')"
          >
            {{ t('actions.accept') }}
          </UButton>
          <UButton
            color="neutral"
            variant="outline"
            icon="i-lucide-x"
            @click.stop="$emit('decline')"
          >
            {{ t('actions.decline') }}
          </UButton>
        </template>

        <!-- Active: admin actions -->
        <template v-else>
          <UButton
            v-if="isAdmin || canInvite"
            color="neutral"
            variant="ghost"
            icon="i-lucide-pencil"
            :title="t('actions.edit')"
            @click.stop="$emit('edit', space)"
          />
          <UDropdownMenu
            v-if="isAdmin || canInvite"
            :items="inviteMenuItems"
          >
            <UButton
              color="primary"
              variant="ghost"
              icon="i-lucide-user-plus"
              :title="t('actions.invite')"
              @click.stop
            />
          </UDropdownMenu>
          <UButton
            v-if="isAdmin"
            color="error"
            variant="ghost"
            icon="i-lucide-trash-2"
            :title="t('actions.delete')"
            @click.stop="$emit('delete', space)"
          />
          <UButton
            v-if="!isAdmin"
            color="warning"
            variant="ghost"
            icon="i-lucide-log-out"
            :title="t('actions.leave')"
            @click.stop="$emit('leave', space)"
          />
        </template>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import type { SpaceWithType } from '@/stores/spaces'
import type { SelectHaexPendingInvites } from '~/database/schemas'

const props = withDefaults(defineProps<{
  space: SpaceWithType
  pending?: boolean
  invite?: SelectHaexPendingInvites
}>(), {
  pending: false,
})

const emit = defineEmits<{
  select: [space: SpaceWithType]
  edit: [space: SpaceWithType]
  'invite-contact': [space: SpaceWithType]
  'invite-link': [space: SpaceWithType]
  delete: [space: SpaceWithType]
  leave: [space: SpaceWithType]
  accept: []
  decline: []
}>()

const inviteMenuItems = computed(() => [
  [{
    label: t('invite.contact'),
    icon: 'i-lucide-user-plus',
    onSelect: () => emit('invite-contact', props.space),
  },
  {
    label: t('invite.link'),
    icon: 'i-lucide-link',
    onSelect: () => emit('invite-link', props.space),
  }],
])

const { t } = useI18n()

const spacesStore = useSpacesStore()
const capabilities = ref<string[]>([])

const isAdmin = computed(() => capabilities.value.includes('space/admin'))
const canInvite = computed(() => capabilities.value.includes('space/invite'))

const permissionLabel = computed(() => {
  if (capabilities.value.includes('space/admin')) return 'Admin'
  if (capabilities.value.includes('space/invite')) return 'Invite'
  if (capabilities.value.includes('space/write')) return 'Write'
  return 'Read'
})

const permissionBadgeColor = computed(() => {
  if (capabilities.value.includes('space/admin')) return 'primary' as const
  if (capabilities.value.includes('space/write')) return 'info' as const
  return 'neutral' as const
})

onMounted(async () => {
  if (!props.pending) {
    capabilities.value = await spacesStore.getCapabilitiesForSpaceAsync(props.space.id)
  }
})

const { getBackendNameByUrl } = useSyncBackendsStore()

const backendName = computed(() => getBackendNameByUrl(props.space.serverUrl))

const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}

const formatCapabilities = (capabilities: string | null): string => {
  if (!capabilities) return ''
  try {
    const parsed = JSON.parse(capabilities) as string[]
    return parsed.map(c => c.replace('space/', '')).join(', ')
  } catch {
    return capabilities
  }
}
</script>

<i18n lang="yaml">
de:
  status:
    pending: Ausstehend
  type:
    local: Lokal
  createdAt: Erstellt am
  actions:
    accept: Annehmen
    decline: Ablehnen
    edit: Bearbeiten
    invite: Einladen
    delete: Löschen
    leave: Verlassen
  invite:
    from: Von
    contact: Kontakt einladen
    link: Einladungslink erstellen
en:
  status:
    pending: Pending
  type:
    local: Local
  createdAt: Created at
  actions:
    accept: Accept
    decline: Decline
    edit: Edit
    invite: Invite
    delete: Delete
    leave: Leave
  invite:
    from: From
    contact: Invite contact
    link: Create invite link
</i18n>
