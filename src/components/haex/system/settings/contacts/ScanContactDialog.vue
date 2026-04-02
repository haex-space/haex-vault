<template>
  <UiDrawerModal
    v-model:open="open"
    :title="currentTitle"
    :description="currentDescription"
  >
    <template #content>
      <!-- Step 1: Scan QR code -->
      <template v-if="step === 'scan'">
        <USelectMenu
          v-if="cameras.length > 1"
          v-model="selectedCameraId"
          :items="cameraOptions"
          value-key="value"
          :placeholder="t('scan.selectCamera')"
          class="w-full mb-3"
        />
        <div
          ref="scannerContainer"
          class="w-full rounded-lg overflow-hidden"
        />
        <p
          v-if="scanError"
          class="text-sm text-red-500 mt-2"
        >
          {{ scanError }}
        </p>
      </template>

      <!-- Step 2: Review scanned contact -->
      <template v-if="step === 'review' && scannedContact">
        <UiInput
          v-model="scannedContact.label"
          :label="t('review.label')"
        />

        <div class="mt-2">
          <label class="text-sm font-medium">{{ t('review.publicKey') }}</label>
          <code
            class="block text-xs text-muted p-2 rounded bg-gray-50 dark:bg-gray-800/50 break-all mt-1"
          >
            {{ scannedContact.publicKey }}
          </code>
        </div>

        <div class="mt-4 space-y-2">
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium">{{ t('review.claims') }}</span>
            <UButton
              variant="outline"
              icon="i-lucide-plus"
              @click="showAddClaimInline = true"
            >
              {{ t('review.addClaim') }}
            </UButton>
          </div>

          <div
            v-for="(claim, index) in scannedContact.claims"
            :key="index"
            class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
          >
            <UiToggle v-model="claim.selected" />
            <div class="min-w-0 flex-1">
              <span class="text-xs font-medium text-muted">
                {{ claim.type }}
              </span>
              <UiInput
                v-model="claim.value"
                class="mt-1"
              />
            </div>
            <UButton
              variant="ghost"
              color="error"
              icon="i-lucide-x"
              @click="scannedContact.claims.splice(index, 1)"
            />
          </div>

          <!-- Inline add claim form -->
          <div
            v-if="showAddClaimInline"
            class="flex items-end gap-2 p-2 rounded border border-dashed border-default"
          >
            <UiInput
              v-model="newClaimType"
              :label="t('review.claimType')"
              placeholder="email, phone, ..."
              class="flex-1"
            />
            <UiInput
              v-model="newClaimValue"
              :label="t('review.claimValue')"
              class="flex-1"
              @keydown.enter.prevent="addInlineClaim"
            />
            <UButton
              icon="i-lucide-check"
              :disabled="!newClaimType.trim() || !newClaimValue.trim()"
              @click="addInlineClaim"
            />
            <UButton
              variant="ghost"
              icon="i-lucide-x"
              @click="showAddClaimInline = false"
            />
          </div>

          <p
            v-if="!scannedContact.claims.length && !showAddClaimInline"
            class="text-xs text-muted"
          >
            {{ t('review.noClaims') }}
          </p>
        </div>

        <UiTextarea
          v-model="contactNotes"
          :label="t('review.notes')"
          :placeholder="t('review.notesPlaceholder')"
          :rows="2"
          class="mt-4"
        />

        <p
          v-if="existingContact"
          class="text-sm text-amber-500 mt-2"
        >
          {{ t('review.alreadyExists', { name: existingContact.label }) }}
        </p>
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <div class="flex gap-2">
          <UButton
            color="neutral"
            variant="outline"
            @click="onClose"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UButton
            v-if="step === 'scan'"
            icon="i-lucide-refresh-cw"
            color="neutral"
            variant="outline"
            :title="t('scan.refreshCameras')"
            @click="refreshCameras"
          />
        </div>
        <UiButton
          v-if="step === 'review'"
          icon="i-lucide-user-plus"
          :loading="isSaving"
          :disabled="!scannedContact?.label.trim() || !!existingContact"
          @click="onSaveContactAsync"
        >
          {{ t('actions.save') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { Html5Qrcode } from 'html5-qrcode'
import type { SelectHaexContacts } from '~/database/schemas'

interface ScannedClaim {
  type: string
  value: string
  selected: boolean
}

interface ScannedContact {
  publicKey: string
  endpointId?: string
  label: string
  claims: ScannedClaim[]
}

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  saved: [contact: SelectHaexContacts]
}>()

const { t } = useI18n()
const { add } = useToast()

const contactsStore = useContactsStore()

const step = ref<'scan' | 'review'>('scan')
const scannerContainer = ref<HTMLElement | null>(null)
const scanError = ref('')
const scannedContact = ref<ScannedContact | null>(null)
const contactNotes = ref('')
const isSaving = ref(false)
const existingContact = ref<SelectHaexContacts | null>(null)
const showAddClaimInline = ref(false)
const newClaimType = ref('')
const newClaimValue = ref('')
const cameras = ref<{ id: string; label: string }[]>([])
const selectedCameraId = ref('')

let scanner: Html5Qrcode | null = null

const cameraOptions = computed(() =>
  cameras.value.map((c) => ({
    label: c.label || c.id,
    value: c.id,
  })),
)

const currentTitle = computed(() =>
  step.value === 'scan' ? t('scan.title') : t('review.title'),
)

const currentDescription = computed(() =>
  step.value === 'scan' ? t('scan.description') : t('review.description'),
)

watch(open, async (isOpen) => {
  if (isOpen) {
    step.value = 'scan'
    scanError.value = ''
    scannedContact.value = null
    contactNotes.value = ''
    existingContact.value = null
    cameras.value = []
    selectedCameraId.value = ''
    await nextTick()
    await loadCameras()
    startScanner()
  } else {
    stopScanner()
  }
})

watch(selectedCameraId, async (newId, oldId) => {
  if (newId && oldId && newId !== oldId) {
    await stopScanner()
    await nextTick()
    startScanner()
  }
})

const loadCameras = async () => {
  try {
    const devices = await Html5Qrcode.getCameras()
    cameras.value = devices.map((d) => ({ id: d.id, label: d.label }))
    if (
      cameras.value.length > 0 &&
      !cameras.value.some((c) => c.id === selectedCameraId.value)
    ) {
      selectedCameraId.value = cameras.value[0]?.id ?? ''
    }
  } catch (error) {
    console.error('Failed to enumerate cameras:', error)
    scanError.value = t('scan.cameraError')
  }
}

const refreshCameras = async () => {
  await stopScanner()
  await loadCameras()
  await nextTick()
  startScanner()
}

const startScanner = async () => {
  if (!scannerContainer.value) return

  const containerId = 'qr-scanner-' + Date.now()
  scannerContainer.value.id = containerId

  const cameraId = selectedCameraId.value

  try {
    scanner = new Html5Qrcode(containerId)
    await scanner.start(
      cameraId || { facingMode: 'environment' },
      { fps: 10, qrbox: { width: 250, height: 250 } },
      onScanSuccess,
      undefined,
    )
  } catch (error) {
    console.error('Failed to start scanner:', error)
    scanError.value = t('scan.cameraError')
  }
}

const stopScanner = async () => {
  if (scanner) {
    try {
      if (scanner.isScanning) {
        await scanner.stop()
      }
    } catch {
      // Scanner might already be stopped
    }
    scanner = null
  }
}

const onScanSuccess = async (decodedText: string) => {
  await stopScanner()

  try {
    const payload = JSON.parse(decodedText)

    if (!payload.publicKey) {
      scanError.value = t('scan.invalidQr')
      step.value = 'scan'
      await nextTick()
      startScanner()
      return
    }

    // Check if contact already exists
    const existing = await contactsStore.getContactByPublicKeyAsync(
      payload.publicKey,
    )
    existingContact.value = existing ?? null

    scannedContact.value = {
      publicKey: payload.publicKey,
      endpointId: payload.endpointId || undefined,
      label: payload.label || '',
      claims: (payload.claims || []).map(
        (c: { type: string; value: string }) => ({
          type: c.type,
          value: c.value,
          selected: true,
        }),
      ),
    }

    contactNotes.value = ''
    step.value = 'review'
  } catch {
    scanError.value = t('scan.invalidQr')
    await nextTick()
    startScanner()
  }
}

const onSaveContactAsync = async () => {
  if (!scannedContact.value || !scannedContact.value.label.trim()) return

  isSaving.value = true
  try {
    const selectedClaims = scannedContact.value.claims
      .filter((c) => c.selected)
      .map((c) => ({ type: c.type, value: c.value }))

    // Persist endpointId as a claim for P2P delivery
    if (scannedContact.value.endpointId) {
      selectedClaims.push({ type: 'endpointId', value: scannedContact.value.endpointId })
    }

    const contact = await contactsStore.addContactWithClaimsAsync(
      scannedContact.value.label.trim(),
      scannedContact.value.publicKey,
      selectedClaims,
      contactNotes.value.trim() || undefined,
    )

    add({ title: t('success.saved'), color: 'success' })
    emit('saved', contact)
    open.value = false
  } catch (error) {
    console.error('Failed to save contact:', error)
    add({
      title: t('errors.saveFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isSaving.value = false
  }
}

const addInlineClaim = () => {
  if (
    !scannedContact.value ||
    !newClaimType.value.trim() ||
    !newClaimValue.value.trim()
  )
    return
  scannedContact.value.claims.push({
    type: newClaimType.value.trim(),
    value: newClaimValue.value.trim(),
    selected: true,
  })
  newClaimType.value = ''
  newClaimValue.value = ''
  showAddClaimInline.value = false
}

const onClose = () => {
  stopScanner()
  open.value = false
}

onBeforeUnmount(() => {
  stopScanner()
})
</script>

<i18n lang="yaml">
de:
  scan:
    title: QR-Code scannen
    description: Scanne den QR-Code der anderen Person, um sie als Kontakt hinzuzufügen
    selectCamera: Kamera auswählen
    refreshCameras: Kameras neu laden
    cameraError: Kamera konnte nicht gestartet werden. Bitte erlaube den Kamerazugriff.
    invalidQr: Ungültiger QR-Code. Bitte scanne einen Identitäts-QR-Code.
  review:
    title: Kontakt prüfen
    description: Prüfe und bearbeite die gescannten Daten, bevor du den Kontakt speicherst
    label: Name
    publicKey: Public Key
    claims: Claims
    addClaim: Hinzufügen
    claimType: Typ
    claimValue: Wert
    noClaims: Keine Claims vorhanden. Du kannst eigene hinzufügen.
    notes: Notizen
    notesPlaceholder: Optionale Notizen zu diesem Kontakt
    alreadyExists: 'Ein Kontakt mit diesem Public Key existiert bereits: {name}'
  actions:
    cancel: Abbrechen
    save: Kontakt speichern
  success:
    saved: Kontakt gespeichert
  errors:
    saveFailed: Kontakt konnte nicht gespeichert werden
en:
  scan:
    title: Scan QR Code
    description: Scan the other person's QR code to add them as a contact
    selectCamera: Select camera
    refreshCameras: Refresh cameras
    cameraError: Could not start camera. Please allow camera access.
    invalidQr: Invalid QR code. Please scan an identity QR code.
  review:
    title: Review Contact
    description: Review and edit the scanned data before saving the contact
    label: Name
    publicKey: Public Key
    claims: Claims
    addClaim: Add
    claimType: Type
    claimValue: Value
    noClaims: No claims yet. You can add your own.
    notes: Notes
    notesPlaceholder: Optional notes about this contact
    alreadyExists: 'A contact with this public key already exists: {name}'
  actions:
    cancel: Cancel
    save: Save Contact
  success:
    saved: Contact saved
  errors:
    saveFailed: Failed to save contact
</i18n>
