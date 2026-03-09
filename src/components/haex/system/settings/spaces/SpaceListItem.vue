<template>
  <div class="flex flex-col gap-2 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50">
    <div class="flex flex-col @xs:flex-row @xs:items-center @xs:justify-between gap-2">
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2 flex-wrap">
          <p class="font-medium text-sm truncate">
            Space {{ space.id.slice(0, 8) }}
          </p>
          <UBadge
            :color="roleBadgeColor"
            variant="subtle"
            size="xs"
          >
            {{ t(`roles.${space.role}`) }}
          </UBadge>
          <UBadge
            v-if="space.canInvite && space.role !== 'admin'"
            color="info"
            variant="subtle"
            size="xs"
          >
            {{ t('canInvite') }}
          </UBadge>
        </div>
        <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
          {{ t('createdAt') }}: {{ formatDate(space.createdAt) }}
        </p>
      </div>
      <div class="flex gap-2 @xs:shrink-0">
        <UButton
          v-if="space.role === 'admin' || space.canInvite"
          color="primary"
          variant="ghost"
          icon="i-lucide-user-plus"
          size="sm"
          :title="t('actions.invite')"
          @click="$emit('invite', space)"
        />
        <UButton
          v-if="space.role === 'admin'"
          color="error"
          variant="ghost"
          icon="i-lucide-trash-2"
          size="sm"
          :title="t('actions.delete')"
          @click="$emit('delete', space)"
        />
        <UButton
          v-if="space.role !== 'admin'"
          color="warning"
          variant="ghost"
          icon="i-lucide-log-out"
          size="sm"
          :title="t('actions.leave')"
          @click="$emit('leave', space)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { SharedSpace } from '@haex-space/vault-sdk'

const props = defineProps<{
  space: SharedSpace
}>()

defineEmits<{
  invite: [space: SharedSpace]
  delete: [space: SharedSpace]
  leave: [space: SharedSpace]
}>()

const { t } = useI18n()

const roleBadgeColor = computed(() => {
  switch (props.space.role) {
    case 'admin': return 'error' as const
    case 'member': return 'primary' as const
    case 'viewer': return 'neutral' as const
  }
})

const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}
</script>

<i18n lang="yaml">
de:
  roles:
    admin: Admin
    member: Mitglied
    viewer: Betrachter
  canInvite: Kann einladen
  createdAt: Erstellt am
  actions:
    invite: Einladen
    delete: Löschen
    leave: Verlassen
en:
  roles:
    admin: Admin
    member: Member
    viewer: Viewer
  canInvite: Can invite
  createdAt: Created at
  actions:
    invite: Invite
    delete: Delete
    leave: Leave
</i18n>
