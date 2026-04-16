<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
  >
    <template #body>
      <div
        v-if="isLoading"
        class="flex items-center justify-center py-8"
      >
        <UIcon
          name="i-lucide-loader-2"
          class="w-5 h-5 animate-spin text-primary"
        />
      </div>

      <div
        v-else-if="ownerIdentity"
        class="space-y-5"
      >
        <!-- Identity -->
        <div class="flex items-center gap-3">
          <UiAvatar
            :src="ownerIdentity.avatar"
            :seed="ownerIdentity.did"
            :avatar-options="parsedAvatarOptions"
            avatar-style="toon-head"
            size="md"
          />
          <div class="min-w-0 flex-1">
            <p class="font-medium truncate">{{ ownerIdentity.name }}</p>
            <div class="flex items-center gap-1 mt-0.5">
              <code class="text-xs text-muted truncate">{{ ownerIdentity.did }}</code>
              <UButton
                variant="ghost"
                color="neutral"
                icon="i-lucide-copy"
                size="xs"
                :title="t('actions.copyDid')"
                @click="copyToClipboard(ownerIdentity!.did)"
              />
            </div>
          </div>
        </div>

        <!-- Identity type badge -->
        <div class="flex items-center gap-2 flex-wrap">
          <UBadge
            v-if="isOwnIdentity"
            color="primary"
            variant="subtle"
            size="sm"
            icon="i-lucide-key-round"
          >
            {{ t('identityType.own') }}
          </UBadge>
          <UBadge
            v-else-if="isContact"
            color="info"
            variant="subtle"
            size="sm"
            icon="i-lucide-book-user"
          >
            {{ t('identityType.contact') }}
          </UBadge>
          <UBadge
            v-else
            color="neutral"
            variant="subtle"
            size="sm"
            icon="i-lucide-user"
          >
            {{ t('identityType.unknown') }}
          </UBadge>
        </div>

        <!-- Claims -->
        <div v-if="claims.length">
          <p class="text-xs font-medium text-muted uppercase tracking-wide mb-2">
            {{ t('sections.claims') }}
          </p>
          <div class="space-y-1">
            <div
              v-for="claim in claims"
              :key="claim.id"
              class="flex items-center justify-between p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <div class="min-w-0 flex-1">
                <span class="text-xs font-medium text-muted">{{ claim.type }}</span>
                <p class="text-sm truncate">{{ claim.value }}</p>
              </div>
              <UButton
                variant="ghost"
                color="neutral"
                icon="i-lucide-copy"
                size="xs"
                @click="copyToClipboard(claim.value)"
              />
            </div>
          </div>
        </div>

        <!-- Notes -->
        <div v-if="ownerIdentity.notes">
          <p class="text-xs font-medium text-muted uppercase tracking-wide mb-1">
            {{ t('sections.notes') }}
          </p>
          <p class="text-sm text-muted">{{ ownerIdentity.notes }}</p>
        </div>

        <!-- Space Origin -->
        <div>
          <p class="text-xs font-medium text-muted uppercase tracking-wide mb-2">
            {{ t('sections.origin') }}
          </p>
          <div class="space-y-2">
            <!-- Server URL -->
            <div
              v-if="space.originUrl"
              class="flex items-center gap-2 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <UIcon name="i-lucide-cloud" class="w-4 h-4 text-muted shrink-0" />
              <div class="min-w-0 flex-1">
                <span class="text-xs font-medium text-muted">{{ t('origin.server') }}</span>
                <p class="text-sm truncate">{{ space.originUrl }}</p>
              </div>
              <UButton
                variant="ghost"
                color="neutral"
                icon="i-lucide-copy"
                size="xs"
                @click="copyToClipboard(space.originUrl)"
              />
            </div>
            <div
              v-else
              class="flex items-center gap-2 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <UIcon name="i-lucide-hard-drive" class="w-4 h-4 text-muted shrink-0" />
              <span class="text-sm text-muted">{{ t('origin.local') }}</span>
            </div>

            <!-- Devices -->
            <div
              v-for="device in ownerDevices"
              :key="device.id"
              class="flex items-center gap-2 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <UIcon name="i-lucide-monitor-smartphone" class="w-4 h-4 text-muted shrink-0" />
              <div class="min-w-0 flex-1">
                <p class="text-sm font-medium truncate">{{ device.deviceName }}</p>
                <div class="flex items-center gap-1">
                  <code class="text-xs text-muted truncate">{{ device.deviceEndpointId }}</code>
                  <UButton
                    variant="ghost"
                    color="neutral"
                    icon="i-lucide-copy"
                    size="xs"
                    @click="copyToClipboard(device.deviceEndpointId)"
                  />
                </div>
                <p
                  v-if="device.relayUrl"
                  class="text-xs text-muted truncate"
                >
                  Relay: {{ device.relayUrl }}
                </p>
              </div>
            </div>
            <p
              v-if="ownerDevices.length === 0"
              class="text-xs text-muted"
            >
              {{ t('origin.noDevices') }}
            </p>
          </div>
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex justify-between gap-4">
        <UiButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ t('actions.close') }}
        </UiButton>
        <UiButton
          v-if="showAddContact"
          icon="i-lucide-book-user"
          color="primary"
          :loading="isAddingContact"
          @click="onAddContactAsync"
        >
          {{ t('actions.addContact') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { eq, and } from 'drizzle-orm'
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import type { SpaceWithType } from '@/stores/spaces'
import type {
  SelectHaexIdentities,
  SelectHaexSpaceDevices,
  SelectHaexIdentityClaims,
} from '~/database/schemas'
import { haexSpaceDevices } from '~/database/schemas'

const props = defineProps<{
  space: SpaceWithType
}>()

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
const { add: addToast } = useToast()

const identityStore = useIdentityStore()
const { currentVault } = storeToRefs(useVaultStore())

const isLoading = ref(true)
const isAddingContact = ref(false)
const ownerIdentity = ref<SelectHaexIdentities | null>(null)
const ownerDevices = ref<SelectHaexSpaceDevices[]>([])
const claims = ref<SelectHaexIdentityClaims[]>([])

const parsedAvatarOptions = computed(() => {
  if (!ownerIdentity.value?.avatarOptions) return null
  try {
    return JSON.parse(ownerIdentity.value.avatarOptions) as Record<string, unknown>
  } catch {
    return null
  }
})

const isOwnIdentity = computed(() =>
  ownerIdentity.value?.privateKey !== null && ownerIdentity.value?.privateKey !== undefined,
)

const isContact = computed(() =>
  !isOwnIdentity.value && ownerIdentity.value?.source === 'contact',
)

const showAddContact = computed(() =>
  ownerIdentity.value && !isOwnIdentity.value && !isContact.value,
)

const loadAsync = async () => {
  isLoading.value = true
  try {
    const identity = await identityStore.getIdentityByIdAsync(props.space.ownerIdentityId)
    ownerIdentity.value = identity ?? null

    if (identity) {
      claims.value = await identityStore.getClaimsAsync(identity.id)
    }

    const db = currentVault.value?.drizzle
    if (db) {
      ownerDevices.value = await db
        .select()
        .from(haexSpaceDevices)
        .where(
          and(
            eq(haexSpaceDevices.spaceId, props.space.id),
            eq(haexSpaceDevices.identityId, props.space.ownerIdentityId),
          ),
        )
    }
  } finally {
    isLoading.value = false
  }
}

const onAddContactAsync = async () => {
  if (!ownerIdentity.value) return
  isAddingContact.value = true
  try {
    const publicKey = await didKeyToPublicKeyAsync(ownerIdentity.value.did)
    await identityStore.addContactAsync(
      ownerIdentity.value.name,
      publicKey,
    )
    addToast({ title: t('success.contactAdded'), color: 'success' })
    // Reload to reflect the new contact status
    await loadAsync()
  } catch (error) {
    addToast({
      title: t('errors.addContactFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isAddingContact.value = false
  }
}

const copyToClipboard = async (text: string) => {
  try {
    await navigator.clipboard.writeText(text)
    addToast({ title: t('success.copied'), color: 'success' })
  } catch {
    addToast({ title: t('errors.copyFailed'), color: 'error' })
  }
}

watch(open, (isOpen) => {
  if (isOpen) loadAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Space-Besitzer
  identityType:
    own: Eigene Identität
    contact: Kontakt
    unknown: Unbekannt
  sections:
    claims: Claims
    notes: Notizen
    origin: Space-Herkunft
  origin:
    server: Server
    local: Nur lokal
    noDevices: Keine Geräte registriert
  actions:
    close: Schließen
    addContact: Als Kontakt hinzufügen
    copyDid: DID kopieren
  success:
    copied: Kopiert
    contactAdded: Kontakt hinzugefügt
  errors:
    copyFailed: Kopieren fehlgeschlagen
    addContactFailed: Kontakt konnte nicht hinzugefügt werden
en:
  title: Space Owner
  identityType:
    own: Own Identity
    contact: Contact
    unknown: Unknown
  sections:
    claims: Claims
    notes: Notes
    origin: Space Origin
  origin:
    server: Server
    local: Local only
    noDevices: No devices registered
  actions:
    close: Close
    addContact: Add to Contacts
    copyDid: Copy DID
  success:
    copied: Copied
    contactAdded: Contact added
  errors:
    copyFailed: Failed to copy
    addContactFailed: Could not add contact
</i18n>
