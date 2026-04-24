<template>
  <div
    :data-testid="`space-card-${space.id}`"
    :data-space-status="pending ? 'pending' : 'active'"
    class="flex flex-col gap-2 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50 transition-colors"
    :class="[
      pending ? 'border border-dashed border-primary/30' : 'cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-700/50',
    ]"
    @click="!pending && $emit('select', space)"
  >
    <!-- Row 1: Name + badges -->
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
        v-if="!pending && space.originUrl"
        color="info"
        variant="subtle"
        size="sm"
        icon="i-lucide-cloud"
      >
        {{ backendName }}
      </UBadge>
      <UBadge
        v-if="!pending && !space.originUrl"
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
      <UBadge
        v-if="!pending && ownerLabel"
        color="neutral"
        variant="subtle"
        size="sm"
        icon="i-lucide-user"
        class="cursor-pointer"
        @click.stop="showOwnerModal = true"
      >
        {{ ownerLabel }}
      </UBadge>
    </div>

    <!-- Pending invite details -->
    <template v-if="pending && invite">
      <!-- Inviter (clickable) -->
      <button
        class="text-xs text-primary hover:underline text-left truncate cursor-pointer"
        @click.stop="showInviteDetail = true"
      >
        {{ t('invite.from') }}: {{ resolvedInviterLabel }}
      </button>

      <!-- Meta row: capabilities, date, expiry -->
      <div class="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted">
        <span v-if="formattedCapabilities">
          <UIcon name="i-lucide-shield" class="w-3 h-3 inline -mt-px" />
          {{ formattedCapabilities }}
        </span>
        <span v-if="invite.createdAt">
          <UIcon name="i-lucide-calendar" class="w-3 h-3 inline -mt-px" />
          {{ formatDate(invite.createdAt) }}
        </span>
      </div>

      <!-- Accept / Decline buttons (own row) -->
      <div class="flex gap-2 pt-1">
        <UiButton
          color="primary"
          variant="soft"
          icon="i-lucide-check"
          @click.stop="$emit('accept')"
        >
          {{ t('actions.accept') }}
        </UiButton>
        <UiButton
          color="neutral"
          variant="outline"
          icon="i-lucide-x"
          @click.stop="$emit('decline')"
        >
          {{ t('actions.decline') }}
        </UiButton>
      </div>
    </template>

    <!-- Active space info -->
    <template v-else>
      <div class="flex items-center justify-between">
        <p class="text-xs text-gray-500 dark:text-gray-400">
          {{ t('createdAt') }}: {{ formatDate(space.createdAt) }}
        </p>
        <div class="flex gap-1">
          <UDropdownMenu
            v-if="canWrite"
            :items="addShareMenuItems"
          >
            <UiButton
              color="primary"
              variant="ghost"
              icon="i-lucide-plus"
              :title="t('actions.addShare')"
              @click.stop
            />
          </UDropdownMenu>
          <UiButton
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
            <UiButton
              color="primary"
              variant="ghost"
              icon="i-lucide-user-plus"
              :title="t('actions.invite')"
              :data-testid="`space-invite-trigger-${space.id}`"
              @click.stop
            />
            <!--
              Custom slots for the dropdown items so each option carries a
              stable per-space data-testid. Click on the inner span bubbles
              up to the Reka UI item container, which fires onSelect — no
              global window hook needed for E2E targeting.
            -->
            <template #invite-option-contact="{ item }">
              <span
                class="flex items-center gap-1.5"
                :data-testid="`space-invite-option-contact-${space.id}`"
              >
                <UIcon
                  v-if="item.icon"
                  :name="item.icon"
                  class="w-4 h-4"
                />
                <span>{{ item.label }}</span>
              </span>
            </template>
            <template #invite-option-link="{ item }">
              <span
                class="flex items-center gap-1.5"
                :data-testid="`space-invite-option-link-${space.id}`"
              >
                <UIcon
                  v-if="item.icon"
                  :name="item.icon"
                  class="w-4 h-4"
                />
                <span>{{ item.label }}</span>
              </span>
            </template>
          </UDropdownMenu>
          <UiButton
            v-if="isAdmin"
            color="error"
            variant="ghost"
            icon="i-lucide-trash-2"
            :title="t('actions.delete')"
            @click.stop="$emit('delete', space)"
          />
          <UiButton
            v-if="!isAdmin"
            color="warning"
            variant="ghost"
            icon="i-lucide-log-out"
            :title="t('actions.leave')"
            @click.stop="$emit('leave', space)"
          />
        </div>
      </div>
    </template>

    <!-- Invite Detail Modal -->
    <UModal
      v-if="pending && invite"
      v-model:open="showInviteDetail"
    >
      <template #content>
        <UCard>
          <template #header>
            <div class="flex items-center justify-between">
              <h3 class="text-base font-semibold">{{ t('detail.title') }}</h3>
              <UiButton
                color="neutral"
                variant="ghost"
                icon="i-lucide-x"
                @click="showInviteDetail = false"
              />
            </div>
          </template>

          <div class="space-y-3 text-sm">
            <!-- Space -->
            <div>
              <p class="text-xs text-muted uppercase tracking-wide mb-1">{{ t('detail.space') }}</p>
              <p class="font-medium">{{ space.name }}</p>
              <p v-if="space.originUrl" class="text-xs text-muted break-all">{{ space.originUrl }}</p>
              <p v-else class="text-xs text-muted">{{ t('type.local') }}</p>
            </div>

            <!-- Inviter -->
            <div>
              <p class="text-xs text-muted uppercase tracking-wide mb-1">{{ t('detail.inviter') }}</p>
              <p class="font-medium">{{ resolvedInviterLabel }}</p>
              <p
                v-if="inviterLabelSource !== 'contact' && invite.inviterLabel"
                class="text-xs text-muted"
              >
                {{ t('detail.senderProvidedLabel') }}: "{{ invite.inviterLabel }}"
              </p>
              <p
                v-if="inviterLabelSource !== 'contact'"
                class="text-xs text-muted italic"
              >
                {{ t('detail.remoteLabelNote') }}
              </p>
              <p class="text-xs text-muted break-all font-mono">{{ invite.inviterDid }}</p>
            </div>

            <!-- Capabilities -->
            <div v-if="formattedCapabilities">
              <p class="text-xs text-muted uppercase tracking-wide mb-1">{{ t('detail.capabilities') }}</p>
              <div class="flex flex-wrap gap-1">
                <UBadge
                  v-for="cap in parsedCapabilities"
                  :key="cap"
                  color="info"
                  variant="subtle"
                  size="sm"
                >
                  {{ cap }}
                </UBadge>
              </div>
            </div>

            <!-- Dates -->
            <div>
              <p class="text-xs text-muted uppercase tracking-wide mb-1">{{ t('detail.dates') }}</p>
              <div class="space-y-1 text-xs">
                <p v-if="invite.createdAt">
                  {{ t('detail.created') }}: {{ formatDate(invite.createdAt) }}
                </p>
                <p v-if="invite.includeHistory" class="text-info">
                  <UIcon name="i-lucide-history" class="w-3 h-3 inline -mt-px" />
                  {{ t('detail.includesHistory') }}
                </p>
              </div>
            </div>

            <!-- Token ID -->
            <div v-if="invite.tokenId">
              <p class="text-xs text-muted uppercase tracking-wide mb-1">{{ t('detail.tokenId') }}</p>
              <p class="text-xs text-muted break-all font-mono">{{ invite.tokenId }}</p>
            </div>
          </div>

          <template #footer>
            <div class="flex gap-2 justify-end">
              <UiButton
                color="neutral"
                variant="outline"
                icon="i-lucide-x"
                @click="showInviteDetail = false; $emit('decline')"
              >
                {{ t('actions.decline') }}
              </UiButton>
              <UiButton
                color="primary"
                icon="i-lucide-check"
                @click="showInviteDetail = false; $emit('accept')"
              >
                {{ t('actions.accept') }}
              </UiButton>
            </div>
          </template>
        </UCard>
      </template>
    </UModal>

    <!-- Space Owner Modal -->
    <SpaceOwnerModal
      v-if="!pending"
      v-model:open="showOwnerModal"
      :space="space"
    />
  </div>
</template>

<script setup lang="ts">
import type { SpaceWithType } from '@/stores/spaces'
import type { SelectHaexPendingInvites } from '~/database/schemas'
import SpaceOwnerModal from './SpaceOwnerModal.vue'

const props = withDefaults(defineProps<{
  space: SpaceWithType
  pending?: boolean
  invite?: SelectHaexPendingInvites
}>(), {
  pending: false,
  invite: undefined,
})

const emit = defineEmits<{
  select: [space: SpaceWithType]
  edit: [space: SpaceWithType]
  'add-share': [payload: { space: SpaceWithType, type: 'folder' | 'file' }]
  'invite-contact': [space: SpaceWithType]
  'invite-link': [space: SpaceWithType]
  delete: [space: SpaceWithType]
  leave: [space: SpaceWithType]
  accept: []
  decline: []
}>()

const { t } = useI18n()

const showInviteDetail = ref(false)
const showOwnerModal = ref(false)

const spacesStore = useSpacesStore()
const identityStore = useIdentityStore()
const { identities, contacts } = storeToRefs(identityStore)
const capabilities = ref<string[]>([])

/**
 * Resolve the inviter display name without blindly trusting the label the
 * sender shipped in the PushInvite. Every vault auto-creates an identity
 * labeled "Meine Identität" / "My Identity", so if we rendered
 * `invite.inviterLabel` directly the recipient would see their *own*
 * default label as the sender — confusing and misleading.
 *
 * Preference order:
 *   1. Local contact entry for the inviter's DID (the receiver named the sender themselves)
 *   2. Truncated DID fallback — always unambiguous, even if ugly
 *
 * The raw `inviterLabel` is only surfaced in the detail modal with an
 * explicit "provided by sender" note.
 */
const inviterLabelSource = computed<'contact' | 'remote' | 'none'>(() => {
  if (!props.invite) return 'none'
  const contact = contacts.value.find(c => c.did === props.invite?.inviterDid)
  if (contact?.name) return 'contact'
  if (props.invite.inviterLabel) return 'remote'
  return 'none'
})

const resolvedInviterLabel = computed(() => {
  if (!props.invite) return ''
  const contact = contacts.value.find(c => c.did === props.invite?.inviterDid)
  if (contact?.name) return contact.name
  return truncateDid(props.invite.inviterDid)
})

const ownerLabel = computed(() => {
  const ownerId = props.space.ownerIdentityId
  if (!ownerId) return ''
  const identity = identities.value.find(i => i.id === ownerId)
  return identity?.name || truncateDid(ownerId)
})

const isAdmin = computed(() => capabilities.value.includes('space/admin'))
const canInvite = computed(() => capabilities.value.includes('space/invite'))
const canWrite = computed(() =>
  capabilities.value.includes('space/admin')
  || capabilities.value.includes('space/write'),
)

const addShareMenuItems = computed(() => [
  [{
    label: t('actions.addFolder'),
    icon: 'i-lucide-folder-plus',
    onSelect: () => emit('add-share', { space: props.space, type: 'folder' }),
  },
  {
    label: t('actions.addFile'),
    icon: 'i-lucide-file-plus',
    onSelect: () => emit('add-share', { space: props.space, type: 'file' }),
  }],
])

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

const inviteMenuItems = computed(() => [
  [{
    label: t('invite.contact'),
    icon: 'i-lucide-user-plus',
    slot: 'invite-option-contact' as const,
    onSelect: () => emit('invite-contact', props.space),
  },
  {
    label: t('invite.link'),
    icon: 'i-lucide-link',
    slot: 'invite-option-link' as const,
    onSelect: () => emit('invite-link', props.space),
  }],
])

onMounted(async () => {
  if (!props.pending) {
    capabilities.value = await spacesStore.getCapabilitiesForSpaceAsync(props.space.id)
  } else {
    // Pending invite: ensure contacts are loaded so resolvedInviterLabel can
    // prefer the local contact name over the untrusted remote label.
    await identityStore.loadIdentitiesAsync()
  }
})

const { getBackendNameByUrl } = useSyncBackendsStore()

const backendName = computed(() => getBackendNameByUrl(props.space.originUrl))

const formatDate = (dateStr: string | null) => {
  if (!dateStr) return ''
  return new Date(dateStr).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

const truncateDid = (did: string) => {
  if (did.length <= 24) return did
  return `${did.slice(0, 20)}…${did.slice(-4)}`
}

const parsedCapabilities = computed((): string[] => {
  if (!props.invite?.capabilities) return []
  try {
    return JSON.parse(props.invite.capabilities) as string[]
  } catch {
    return []
  }
})

const formattedCapabilities = computed(() => {
  return parsedCapabilities.value.map(c => c.replace('space/', '')).join(', ')
})
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
    addShare: Datei oder Ordner hinzufügen
    addFolder: Ordner hinzufügen
    addFile: Datei hinzufügen
  invite:
    from: Von
    contact: Kontakt einladen
    link: Einladungslink erstellen
  detail:
    title: Einladungsdetails
    space: Space
    inviter: Einladender
    senderProvidedLabel: Vom Absender angegeben
    remoteLabelNote: Füge diese Identität als Kontakt hinzu, um einen eigenen Namen zu vergeben.
    capabilities: Berechtigungen
    dates: Details
    created: Erstellt
    includesHistory: Enthält Verlauf
    tokenId: Token-ID
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
    addShare: Add file or folder
    addFolder: Add folder
    addFile: Add file
  invite:
    from: From
    contact: Invite contact
    link: Create invite link
  detail:
    title: Invitation details
    space: Space
    inviter: Invited by
    senderProvidedLabel: Provided by sender
    remoteLabelNote: Add this identity as a contact to assign your own label.
    capabilities: Permissions
    dates: Details
    created: Created
    includesHistory: Includes history
    tokenId: Token ID
</i18n>
