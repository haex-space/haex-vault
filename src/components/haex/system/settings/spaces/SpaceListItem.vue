<template>
  <div
    class="flex flex-col gap-2 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50"
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
            v-if="space.serverUrl"
            color="info"
            variant="subtle"
            size="sm"
            icon="i-lucide-cloud"
          >
            {{ backendName }}
          </UBadge>
          <UBadge
            v-if="federationOriginHost"
            color="warning"
            variant="subtle"
            size="sm"
            icon="i-lucide-globe"
          >
            {{ t('federation.label') }} {{ federationOriginHost }}
          </UBadge>
          <UBadge
            :color="roleBadgeColor"
            variant="subtle"
            size="sm"
          >
            {{ t(`roles.${space.role}`) }}
          </UBadge>
        </div>
        <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
          {{ t('createdAt') }}: {{ formatDate(space.createdAt) }}
        </p>
      </div>
      <div class="flex gap-2 @xs:shrink-0">
        <UButton
          v-if="space.role === SpaceRoles.ADMIN || space.role === SpaceRoles.OWNER"
          color="neutral"
          variant="ghost"
          icon="i-lucide-pencil"
          :title="t('actions.edit')"
          @click="$emit('edit', space)"
        />
        <UDropdownMenu
          v-if="space.role === SpaceRoles.ADMIN || space.role === SpaceRoles.OWNER"
          :items="inviteMenuItems"
        >
          <UButton
            color="primary"
            variant="ghost"
            icon="i-lucide-user-plus"
            :title="t('actions.invite')"
          />
        </UDropdownMenu>
        <UButton
          v-if="space.role === SpaceRoles.ADMIN || space.role === SpaceRoles.OWNER"
          color="error"
          variant="ghost"
          icon="i-lucide-trash-2"
          :title="t('actions.delete')"
          @click="$emit('delete', space)"
        />
        <UButton
          v-if="space.role !== SpaceRoles.ADMIN"
          color="warning"
          variant="ghost"
          icon="i-lucide-log-out"
          :title="t('actions.leave')"
          @click="$emit('leave', space)"
        />
      </div>
    </div>

    <!-- Linked items collapsible -->
    <UCollapsible v-model:open="isExpanded" :unmount-on-hide="false">
      <div class="flex items-center gap-1.5 py-2 text-xs text-muted hover:text-foreground transition-colors cursor-pointer">
        <UIcon
          name="i-lucide-chevron-right"
          class="w-3.5 h-3.5 transition-transform duration-200"
          :class="{ 'rotate-90': isExpanded }"
        />
        <span>{{ t('linkedItems.label') }}</span>
        <UBadge
          v-if="totalCount > 0"
          variant="subtle"
          size="sm"
          color="neutral"
        >
          {{ totalCount }}
        </UBadge>
      </div>

      <template #content>
        <div class="mt-2">
          <!-- Federation Info -->
          <div
            v-if="federationBackend?.type === 'relay'"
            class="mb-3 p-2 rounded-md bg-warning-50 dark:bg-warning-950/30 text-xs space-y-1"
          >
            <p class="font-medium text-warning-700 dark:text-warning-300 flex items-center gap-1.5">
              <UIcon name="i-lucide-globe" class="w-3.5 h-3.5" />
              {{ t('federation.title') }}
            </p>
            <div class="grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5 text-gray-600 dark:text-gray-400">
              <span>{{ t('federation.originServer') }}:</span>
              <span class="truncate font-mono">{{ space.serverUrl }}</span>
              <span>{{ t('federation.relayServer') }}:</span>
              <span class="truncate font-mono">{{ federationBackend.homeServerUrl }}</span>
              <span v-if="federationBackend.originServerDid">{{ t('federation.originDid') }}:</span>
              <span v-if="federationBackend.originServerDid" class="truncate font-mono">{{ federationBackend.originServerDid }}</span>
              <span v-if="federationBackend.homeServerDid">{{ t('federation.relayDid') }}:</span>
              <span v-if="federationBackend.homeServerDid" class="truncate font-mono">{{ federationBackend.homeServerDid }}</span>
            </div>
          </div>
          <SpaceLinkedItems
            :groups="groups"
            :is-loading="isLoading"
          />
        </div>
      </template>
    </UCollapsible>
  </div>
</template>

<script setup lang="ts">
import { SpaceRoles, type DecryptedSpace } from '@haex-space/vault-sdk'
import SpaceLinkedItems from './SpaceLinkedItems.vue'

const props = defineProps<{
  space: DecryptedSpace
}>()

const emit = defineEmits<{
  edit: [space: DecryptedSpace]
  'invite-contact': [space: DecryptedSpace]
  'invite-link': [space: DecryptedSpace]
  'invite-open': [space: DecryptedSpace]
  delete: [space: DecryptedSpace]
  leave: [space: DecryptedSpace]
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
  },
  {
    label: t('invite.open'),
    icon: 'i-lucide-qr-code',
    onSelect: () => emit('invite-open', props.space),
  }],
])

const { t } = useI18n()

const { getBackendNameByUrl, isFederated, getBackendForSpace } = useSyncBackendsStore()

const backendName = computed(() => getBackendNameByUrl(props.space.serverUrl))

const federationBackend = computed(() => getBackendForSpace(props.space.id))

const federationOriginHost = computed(() => {
  if (!federationBackend.value || federationBackend.value.type !== 'relay') return null
  try {
    return new URL(props.space.serverUrl).hostname
  } catch {
    return props.space.serverUrl
  }
})

const roleBadgeColor = computed(() => {
  switch (props.space.role) {
    case 'admin':
      return 'primary' as const
    case 'owner':
      return 'warning' as const
    case 'member':
      return 'info' as const
    case 'reader':
      return 'neutral' as const
    default:
      return 'neutral' as const
  }
})

const formatDate = (dateStr: string) => {
  return new Date(dateStr).toLocaleDateString()
}

// Linked items
const isExpanded = ref(false)
const hasLoaded = ref(false)

const { groups, totalCount, isLoading, loadAsync } = useSpaceLinkedItems(
  () => props.space.id,
)

watch(isExpanded, async (expanded) => {
  if (expanded && !hasLoaded.value) {
    await loadAsync()
    hasLoaded.value = true
  }
})
</script>

<i18n lang="yaml">
de:
  roles:
    admin: Admin
    owner: Eigentümer
    member: Mitglied
    reader: Leser
  createdAt: Erstellt am
  actions:
    edit: Bearbeiten
    invite: Einladen
    delete: Löschen
    leave: Verlassen
  invite:
    contact: Kontakt einladen
    link: Einladungslink erstellen
    open: Offene Einladung
  federation:
    label: "Föderiert:"
    title: Föderation
    originServer: Origin-Server
    relayServer: Relay-Server
    originDid: Origin-DID
    relayDid: Relay-DID
  linkedItems:
    label: Verknüpfte Inhalte
en:
  roles:
    admin: Admin
    owner: Owner
    member: Member
    reader: Reader
  createdAt: Created at
  actions:
    edit: Edit
    invite: Invite
    delete: Delete
    leave: Leave
  invite:
    contact: Invite contact
    link: Create invite link
    open: Open invitation
  federation:
    label: "Federated:"
    title: Federation
    originServer: Origin server
    relayServer: Relay server
    originDid: Origin DID
    relayDid: Relay DID
  linkedItems:
    label: Linked content
</i18n>
