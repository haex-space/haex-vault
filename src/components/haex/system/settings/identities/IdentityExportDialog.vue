<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <!-- Avatar -->
      <div
        v-if="target?.avatar"
        class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
      >
        <UCheckbox v-model="includeAvatar" />
        <UiAvatar
          :src="target.avatar"
          :seed="target.did"
          avatar-style="toon-head"
          size="sm"
        />
        <span class="text-sm">{{ t('includeAvatar') }}</span>
      </div>

      <!-- Claims selection -->
      <div
        v-if="claims.length"
        class="space-y-2"
      >
        <span class="text-sm font-medium">{{ t('selectClaims') }}</span>
        <div
          v-for="claim in claims"
          :key="claim.id"
          class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
        >
          <UCheckbox
            :model-value="selectedClaimIds.has(claim.id)"
            @update:model-value="toggleClaim(claim.id)"
          />
          <div class="min-w-0 flex-1">
            <span class="text-xs font-medium text-muted">{{ claim.type }}</span>
            <p class="text-sm truncate">{{ claim.value }}</p>
          </div>
        </div>
      </div>
      <p
        v-else
        class="text-sm text-muted"
      >
        {{ t('noClaims') }}
      </p>

      <!-- Private key (hidden behind collapsible) -->
      <UCollapsible class="mt-4">
        <div class="flex items-center gap-2 cursor-pointer text-sm text-muted">
          <UIcon
            name="i-lucide-chevron-right"
            class="w-4 h-4 shrink-0 transition-transform duration-200"
            :class="{ 'rotate-90': includePrivateKey }"
          />
          <UIcon
            name="i-lucide-shield-alert"
            class="w-4 h-4 text-red-500"
          />
          <span>{{ t('advancedSection') }}</span>
        </div>
        <template #content>
          <div
            class="mt-2 p-3 rounded-lg border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-950/30"
          >
            <div class="flex items-start gap-3">
              <UCheckbox
                v-model="includePrivateKey"
                color="error"
              />
              <div>
                <span
                  class="text-sm font-medium text-red-700 dark:text-red-400"
                  >{{ t('includePrivateKey') }}</span
                >
                <p class="text-xs text-red-600 dark:text-red-500 mt-1">
                  {{ t('privateKeyWarning') }}
                </p>
              </div>
            </div>
          </div>
        </template>
      </UCollapsible>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UiButton
          icon="i-lucide-download"
          :loading="submitting"
          @click="onSubmit"
        >
          {{ t('submit') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'
import type { ExportClaim } from '@/composables/useIdentityExport'

export interface ExportSubmitPayload {
  selectedClaimIds: Set<string>
  includeAvatar: boolean
  includePrivateKey: boolean
}

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  target: SelectHaexIdentities | null
  claims: ExportClaim[]
  submitting: boolean
}>()

const emit = defineEmits<{
  submit: [payload: ExportSubmitPayload]
}>()

const { t } = useI18n()

const includeAvatar = ref(true)
const includePrivateKey = ref(false)
const selectedClaimIds = ref(new Set<string>())

// Seed selection on open — all claims selected by default, avatar mirrors
// the target, private key always starts off.
watch(
  () => [open.value, props.target, props.claims] as const,
  ([isOpen, target, claims]) => {
    if (!isOpen) return
    includeAvatar.value = !!target?.avatar
    includePrivateKey.value = false
    selectedClaimIds.value = new Set(claims.map((c) => c.id))
  },
)

const toggleClaim = (claimId: string) => {
  if (selectedClaimIds.value.has(claimId)) {
    selectedClaimIds.value.delete(claimId)
  } else {
    selectedClaimIds.value.add(claimId)
  }
}

const onSubmit = () => {
  emit('submit', {
    selectedClaimIds: new Set(selectedClaimIds.value),
    includeAvatar: includeAvatar.value,
    includePrivateKey: includePrivateKey.value,
  })
}
</script>

<i18n lang="yaml">
de:
  title: Identität exportieren
  description: Wähle aus, welche Daten in die exportierte Datei aufgenommen werden sollen.
  selectClaims: Claims zum Exportieren auswählen
  noClaims: Keine Claims vorhanden. Nur der Public Key wird exportiert.
  includeAvatar: Profilbild übernehmen
  advancedSection: Erweitert — Privater Schlüssel
  includePrivateKey: Privaten Schlüssel exportieren
  privateKeyWarning: Nur für vollständige Backups nutzen. Wer die Datei erhält, kann deine Identität vollständig übernehmen.
  submit: Datei speichern
  cancel: Abbrechen
en:
  title: Export Identity
  description: Choose which data should be included in the exported file.
  selectClaims: Select claims to export
  noClaims: No claims available. Only the public key will be exported.
  includeAvatar: Include avatar
  advancedSection: Advanced — Private key
  includePrivateKey: Export private key
  privateKeyWarning: Only use for full backups. Anyone with this file can fully impersonate your identity.
  submit: Save file
  cancel: Cancel
</i18n>
