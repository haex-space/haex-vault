<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #content>
      <!-- Step 1: Select identity and claims -->
      <template v-if="!qrDataUrl">
        <USelectMenu
          v-model="selectedIdentityId"
          :items="identityOptions"
          value-key="value"
          :placeholder="t('selectIdentity')"
          class="w-full"
        />

        <div
          v-if="selectedIdentityId && availableClaims.length"
          class="mt-4 space-y-2"
        >
          <span class="text-sm font-medium">{{ t('selectClaims') }}</span>
          <div
            v-for="claim in availableClaims"
            :key="claim.id"
            class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
          >
            <UiToggle
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
          v-else-if="selectedIdentityId && !availableClaims.length"
          class="mt-4 text-sm text-muted"
        >
          {{ t('noClaims') }}
        </p>
      </template>

      <!-- Step 2: Show QR code -->
      <template v-else>
        <div class="flex flex-col items-center gap-4">
          <img
            :src="qrDataUrl"
            :alt="t('qrAlt')"
            class="w-64 h-64 rounded-lg border border-default"
          />
          <p class="text-sm text-muted text-center">
            {{ t('scanHint') }}
          </p>
        </div>
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="onClose"
        >
          {{ qrDataUrl ? t('actions.close') : t('actions.cancel') }}
        </UButton>
        <UiButton
          v-if="!qrDataUrl"
          icon="i-lucide-qr-code"
          :loading="isGenerating"
          :disabled="!selectedIdentityId"
          @click="generateQrAsync"
        >
          {{ t('actions.generate') }}
        </UiButton>
        <UiButton
          v-else
          icon="i-lucide-arrow-left"
          variant="outline"
          @click="qrDataUrl = ''"
        >
          {{ t('actions.back') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import QRCode from 'qrcode'

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { add } = useToast()

const identityStore = useIdentityStore()
const { identities } = storeToRefs(identityStore)

const selectedIdentityId = ref<string>('')
const selectedClaimIds = ref(new Set<string>())
const availableClaims = ref<{ id: string; type: string; value: string }[]>([])
const qrDataUrl = ref('')
const isGenerating = ref(false)

const identityOptions = computed(() =>
  identities.value.map(i => ({
    label: i.label,
    value: i.id,
  })),
)

watch(selectedIdentityId, async (id) => {
  selectedClaimIds.value.clear()
  availableClaims.value = []
  if (!id) return

  const claims = await identityStore.getClaimsAsync(id)
  availableClaims.value = claims.map(c => ({ id: c.id, type: c.type, value: c.value }))
  // Select all claims by default
  for (const claim of availableClaims.value) {
    selectedClaimIds.value.add(claim.id)
  }
})

watch(open, (isOpen) => {
  if (isOpen) {
    selectedIdentityId.value = ''
    selectedClaimIds.value.clear()
    availableClaims.value = []
    qrDataUrl.value = ''
    identityStore.loadIdentitiesAsync()
  }
})

const toggleClaim = (claimId: string) => {
  if (selectedClaimIds.value.has(claimId)) {
    selectedClaimIds.value.delete(claimId)
  } else {
    selectedClaimIds.value.add(claimId)
  }
}

const generateQrAsync = async () => {
  if (!selectedIdentityId.value) return

  isGenerating.value = true
  try {
    const identity = identities.value.find(i => i.id === selectedIdentityId.value)
    if (!identity) throw new Error('Identity not found')

    const selectedClaims = availableClaims.value
      .filter(c => selectedClaimIds.value.has(c.id))
      .map(c => ({ type: c.type, value: c.value }))

    const payload = {
      v: 1,
      publicKey: identity.publicKey,
      label: identity.label,
      claims: selectedClaims,
    }

    qrDataUrl.value = await QRCode.toDataURL(JSON.stringify(payload), {
      width: 512,
      margin: 2,
      errorCorrectionLevel: 'M',
    })
  } catch (error) {
    console.error('Failed to generate QR code:', error)
    add({
      title: t('errors.generateFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isGenerating.value = false
  }
}

const onClose = () => {
  open.value = false
}
</script>

<i18n lang="yaml">
de:
  title: Identität teilen
  description: Erstelle einen QR-Code mit deiner Identität und ausgewählten Claims
  selectIdentity: Identität auswählen
  selectClaims: Claims zum Teilen auswählen
  noClaims: Keine Claims vorhanden. Du kannst trotzdem deinen Public Key teilen.
  qrAlt: QR-Code mit Identitätsdaten
  scanHint: Lass die andere Person diesen QR-Code scannen, um dich als Kontakt hinzuzufügen.
  actions:
    generate: QR-Code erstellen
    cancel: Abbrechen
    close: Schließen
    back: Zurück
  errors:
    generateFailed: QR-Code konnte nicht erstellt werden
en:
  title: Share Identity
  description: Create a QR code with your identity and selected claims
  selectIdentity: Select identity
  selectClaims: Select claims to share
  noClaims: No claims available. You can still share your public key.
  qrAlt: QR code with identity data
  scanHint: Let the other person scan this QR code to add you as a contact.
  actions:
    generate: Generate QR Code
    cancel: Cancel
    close: Close
    back: Back
  errors:
    generateFailed: Failed to generate QR code
</i18n>
