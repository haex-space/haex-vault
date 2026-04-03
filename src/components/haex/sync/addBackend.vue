<template>
  <div class="space-y-4">
    <!-- Server URL Selection -->
    <div class="flex flex-col space-y-2">
      <USelectMenu
        v-model="selectedServerOption"
        :items
        class="w-full"
      >
        <template #item="{ item }">
          <UUser
            :name="item.label"
            :description="item.value"
          />
        </template>
      </USelectMenu>

      <UiInput
        v-if="selectedServerOption.value === 'custom'"
        v-model="customServerUrl"
        :label="t('customUrl.label')"
        class="w-full"
        data-testid="sync-custom-url-input"
      />
    </div>

    <!-- Identity Selector -->
    <div class="flex flex-col space-y-2">
      <label class="text-sm font-medium">{{ t('identity.label') }}</label>
      <div class="flex gap-2">
        <USelectMenu
          v-model="selectedIdentityId"
          :items="identityOptions"
          value-key="value"
          class="flex-1"
          :placeholder="t('identity.placeholder')"
        />
        <UButton
          icon="i-lucide-user"
          color="neutral"
          variant="outline"
          :title="t('identity.manage')"
          @click="navigateToIdentities"
        />
      </div>
      <p
        v-if="ownIdentities.length === 0"
        class="text-xs text-amber-500"
      >
        {{ t('identity.noIdentities') }}
      </p>
    </div>

    <!-- Requirements Loading -->
    <div
      v-if="isLoadingRequirements"
      class="flex items-center justify-center py-4"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-5 h-5 animate-spin text-primary"
      />
    </div>

    <!-- Requirements Error -->
    <UAlert
      v-if="requirementsError"
      color="error"
      icon="i-lucide-alert-circle"
      :description="requirementsError"
    />

    <!-- Server Requirements Display -->
    <div
      v-if="requirements"
      class="space-y-3"
    >
      <p class="text-sm text-muted">
        {{ t('requirements.description') }}
      </p>

      <!-- Claim Consent Checkboxes -->
      <div
        v-for="claim in requirements.claims"
        :key="claim.type"
        class="flex items-start gap-3 p-3 rounded-lg bg-gray-50 dark:bg-gray-800/50"
      >
        <UCheckbox
          :model-value="claimApproval[claim.type] ?? false"
          :disabled="claim.required"
          @update:model-value="toggleClaim(claim.type, $event as boolean)"
        />
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-sm font-medium">{{ claim.label }}</span>
            <UBadge
              v-if="claim.required"
              color="error"
              variant="subtle"
            >
              {{ t('requirements.required') }}
            </UBadge>
          </div>
          <p class="text-xs text-muted mt-1">
            {{ t('requirements.claimType') }}: {{ claim.type }}
          </p>
          <p
            v-if="matchedClaims[claim.type]"
            class="text-xs text-success mt-1"
          >
            {{ t('requirements.value') }}: {{ matchedClaims[claim.type] }}
          </p>
          <p
            v-else
            class="text-xs text-amber-500 mt-1"
          >
            {{ t('requirements.missingClaim') }}
          </p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { SettingsCategory } from '~/config/settingsCategories'
import type { ServerRequirements } from '~/composables/useCreateSyncConnection'

const { t } = useI18n()

defineProps<{
  items: ISyncServerOption[]
  autofocus?: boolean
}>()

const serverUrl = defineModel<string>('serverUrl')
const identityId = defineModel<string>('identityId')
const approvedClaims = defineModel<Record<string, string>>('approvedClaims')

const windowManager = useWindowManagerStore()
const identityStore = useIdentityStore()
const { ownIdentities } = storeToRefs(identityStore)

// Requirements state (declared early — used by watchers below)
const requirements = ref<ServerRequirements | null>(null)
const requirementsError = ref<string | null>(null)
const isLoadingRequirements = ref(false)
const claimApproval = ref<Record<string, boolean>>({})

// Load identities on mount
onMounted(async () => {
  await identityStore.loadIdentitiesAsync()
})

// Identity options for selector
const identityOptions = computed(() =>
  ownIdentities.value.map((id) => ({
    label: `${id.label} (${id.did.slice(0, 24)}...)`,
    value: id.id,
  })),
)

const selectedIdentityId = computed({
  get: () => identityId.value,
  set: (val) => {
    identityId.value = val
    // Reset requirements when identity changes
    requirements.value = null
    requirementsError.value = null
  },
})

// Auto-fetch requirements when both identity and server URL are set
watch(
  [() => identityId.value, () => serverUrl.value],
  async ([newIdentityId, newServerUrl]) => {
    if (newIdentityId && newServerUrl) {
      await checkRequirementsAsync()
    }
  },
)

const navigateToIdentities = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Identities },
  })
}

// Server URL handling
const defaultServerOption: ISyncServerOption = {
  label: 'HaexSpace',
  value: 'https://sync.haex.space',
}

const selectedServerOption = ref<ISyncServerOption>(defaultServerOption)
const customServerUrl = ref()

watch(
  [customServerUrl, selectedServerOption],
  () => {
    if (selectedServerOption.value.value === 'custom') {
      serverUrl.value = customServerUrl.value
    } else {
      customServerUrl.value = ''
      serverUrl.value = selectedServerOption.value.value
    }
    // Reset requirements when server changes
    requirements.value = null
    requirementsError.value = null
  },
  { immediate: true },
)

const { fetchRequirementsAsync } = useCreateSyncConnection()

// Matched claims from identity
const matchedClaims = computed<Record<string, string>>(() => {
  // We'll need the claims from the selected identity — load them reactively
  return identityClaimsMap.value
})

// Identity claims loaded separately
const identityClaimsMap = ref<Record<string, string>>({})

watch(
  () => identityId.value,
  async (newId) => {
    if (!newId) {
      identityClaimsMap.value = {}
      return
    }
    const claims = await identityStore.getClaimsAsync(newId)
    const map: Record<string, string> = {}
    for (const c of claims) {
      map[c.type] = c.value
    }
    identityClaimsMap.value = map
  },
  { immediate: true },
)

// Check requirements
const checkRequirementsAsync = async () => {
  if (!serverUrl.value) return
  isLoadingRequirements.value = true
  requirementsError.value = null

  try {
    // Reload claims fresh (user may have added claims since last load)
    if (identityId.value) {
      const claims = await identityStore.getClaimsAsync(identityId.value)
      const map: Record<string, string> = {}
      for (const c of claims) {
        map[c.type] = c.value
      }
      identityClaimsMap.value = map
    }

    const reqs = await fetchRequirementsAsync(serverUrl.value)
    requirements.value = reqs

    // Auto-approve required claims and pre-approve optional ones that we have
    const approval: Record<string, boolean> = {}
    for (const claim of reqs.claims) {
      if (claim.required) {
        approval[claim.type] = true
      } else {
        approval[claim.type] = !!identityClaimsMap.value[claim.type]
      }
    }
    claimApproval.value = approval

    updateApprovedClaims()
  } catch (e) {
    requirementsError.value = e instanceof Error ? e.message : 'Unknown error'
  } finally {
    isLoadingRequirements.value = false
  }
}

// Toggle claim approval
const toggleClaim = (type: string, approved: boolean) => {
  claimApproval.value[type] = approved
  updateApprovedClaims()
}

// Update the approved claims model
const updateApprovedClaims = () => {
  const result: Record<string, string> = {}
  for (const [type, approved] of Object.entries(claimApproval.value)) {
    if (approved && identityClaimsMap.value[type]) {
      result[type] = identityClaimsMap.value[type]
    }
  }
  approvedClaims.value = result
}
</script>

<i18n lang="yaml">
de:
  serverUrl:
    label: Server-URL
    description: Wähle einen vorkonfigurierten Server oder gib eine benutzerdefinierte URL ein
  customUrl:
    label: Benutzerdefinierte Server-URL
    description: Gib die URL deines eigenen Sync-Servers ein
    placeholder: https://dein-server.de
  identity:
    label: Identität
    placeholder: Identität auswählen...
    manage: Identitäten verwalten
    noIdentities: Keine Identitäten vorhanden. Erstelle zuerst eine Identität in den Einstellungen.
  requirements:
    check: Server-Anforderungen prüfen
    description: Der Server benötigt folgende Informationen. Wähle aus, welche Daten du teilen möchtest.
    required: Pflicht
    claimType: Typ
    value: Wert
    missingClaim: Kein passender Claim vorhanden
  actions:
    connect: Verbinden
    cancel: Abbrechen

en:
  serverUrl:
    label: Server URL
    description: Choose a preconfigured server or enter a custom URL
  customUrl:
    label: Custom Server URL
    description: Enter the URL of your own sync server
    placeholder: https://your-server.com
  identity:
    label: Identity
    placeholder: Select identity...
    manage: Manage identities
    noIdentities: No identities found. Create an identity in settings first.
  requirements:
    check: Check Server Requirements
    description: The server requires the following information. Choose which data you want to share.
    required: Required
    claimType: Type
    value: Value
    missingClaim: No matching claim available
  actions:
    connect: Connect
    cancel: Cancel
</i18n>
