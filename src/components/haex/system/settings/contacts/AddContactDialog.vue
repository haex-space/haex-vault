<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <UTabs
        v-model="wizard.addMode.value"
        :items="wizard.addTabItems.value"
        class="w-full"
        data-testid="contacts-add-tabs"
      />

      <ModeFile
        v-if="wizard.addMode.value === 'file'"
        :import-json="wizard.importJson.value"
        :import-parsed="wizard.importParsed.value"
        :import-selected-claim-indices="wizard.importSelectedClaimIndices.value"
        :import-include-avatar="wizard.importIncludeAvatar.value"
        @update:import-json="wizard.importJson.value = $event"
        @update:import-include-avatar="wizard.importIncludeAvatar.value = $event"
        @select-file="wizard.onSelectImportFileAsync"
        @toggle-claim="wizard.toggleImportClaim"
      />

      <ModeManual
        v-else-if="wizard.addMode.value === 'manual'"
        :form="wizard.manualForm"
      />

      <ModeScan
        v-else-if="wizard.addMode.value === 'scan'"
        ref="scanRef"
        :active="wizard.addMode.value === 'scan'"
        :scan-step="wizard.scanStep.value"
        :scanned-contact="wizard.scannedContact.value"
        :scan-contact-notes="wizard.scanContactNotes.value"
        :scan-error="wizard.scanError.value"
        :scan-blocking-identity="wizard.scanBlockingIdentity.value"
        :scan-show-add-claim-inline="wizard.scanShowAddClaimInline.value"
        :scan-new-claim-type="wizard.scanNewClaimType.value"
        :scan-new-claim-value="wizard.scanNewClaimValue.value"
        @update:scan-step="wizard.scanStep.value = $event"
        @update:scan-error="wizard.scanError.value = $event"
        @update:scan-contact-notes="wizard.scanContactNotes.value = $event"
        @update:scan-show-add-claim-inline="wizard.scanShowAddClaimInline.value = $event"
        @update:scan-new-claim-type="wizard.scanNewClaimType.value = $event"
        @update:scan-new-claim-value="wizard.scanNewClaimValue.value = $event"
        @ingest="wizard.ingestScannedPayload"
        @add-inline-claim="wizard.addScanInlineClaim"
      />
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <div class="flex gap-2">
          <UButton
            color="neutral"
            variant="outline"
            @click="onBack"
          >
            {{ wizard.backLabel.value }}
          </UButton>
          <UiButton
            v-if="wizard.addMode.value === 'scan' && wizard.scanStep.value === 'scan'"
            icon="i-lucide-refresh-cw"
            color="neutral"
            variant="outline"
            :title="t('scan.refreshCameras')"
            @click="scanRef?.refreshScanCameras"
          />
        </div>

        <!-- File mode buttons -->
        <template v-if="wizard.addMode.value === 'file'">
          <UiButton
            v-if="!wizard.importParsed.value"
            icon="i-lucide-arrow-right"
            :disabled="!wizard.importJson.value.trim()"
            data-testid="contacts-import-preview"
            @click="wizard.onParseImport"
          >
            {{ t('file.preview') }}
          </UiButton>
          <UiButton
            v-else
            icon="i-lucide-plus"
            :loading="wizard.isAdding.value"
            data-testid="contacts-import-submit"
            @click="wizard.onImportContactAsync"
          >
            {{ t('actions.add') }}
          </UiButton>
        </template>

        <!-- Manual mode button -->
        <UiButton
          v-else-if="wizard.addMode.value === 'manual'"
          icon="i-lucide-plus"
          :loading="wizard.isAdding.value"
          :disabled="!wizard.manualForm.label.trim() || !wizard.manualForm.publicKey.trim()"
          @click="wizard.onAddManualContactAsync"
        >
          {{ t('actions.add') }}
        </UiButton>

        <!-- Scan mode: save button (only in review step) -->
        <UiButton
          v-else-if="wizard.addMode.value === 'scan' && wizard.scanStep.value === 'review'"
          icon="i-lucide-user-plus"
          :loading="wizard.scanIsSaving.value"
          :disabled="!wizard.scannedContact.value?.name.trim() || !!wizard.scanBlockingIdentity.value"
          @click="wizard.onSaveScanContactAsync"
        >
          {{ t('actions.add') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import ModeFile from './AddContactDialog/ModeFile.vue'
import ModeManual from './AddContactDialog/ModeManual.vue'
import ModeScan from './AddContactDialog/ModeScan.vue'
import { useAddContactWizard } from '@/composables/contacts/useAddContactWizard'

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  added: []
}>()

const { t } = useI18n()

const scanRef = ref<InstanceType<typeof ModeScan> | null>(null)

const wizard = useAddContactWizard({
  open,
  onAdded: () => emit('added'),
})

const onBack = async () => {
  if (wizard.addMode.value === 'file' && wizard.importParsed.value) {
    wizard.importParsed.value = null
  } else if (wizard.addMode.value === 'scan' && wizard.scanStep.value === 'review') {
    wizard.resetScanState()
    await nextTick()
    scanRef.value?.startQrScanner()
  } else {
    open.value = false
  }
}

// --- Dialog open/close ---
watch(open, async (isOpen) => {
  if (isOpen) {
    wizard.addMode.value = ''
    wizard.resetFileImport()
    wizard.resetManualForm()
    wizard.resetScanState()
    wizard.addMode.value = 'scan'
  } else {
    await scanRef.value?.stopQrScanner()
  }
})

watch(() => wizard.addMode.value, async (newMode, oldMode) => {
  if (oldMode === 'scan') await scanRef.value?.stopQrScanner()
  if (newMode === 'scan' && open.value) {
    wizard.resetScanState()
  }
})
</script>

<i18n lang="yaml">
de:
  title: Kontakt hinzufügen
  description: Scanne einen QR-Code, importiere aus einer Datei oder füge manuell hinzu
  tabs:
    scan: QR-Code
    file: Aus Datei
    manual: Manuell
  file:
    selectFile: JSON-Datei auswählen
    orPaste: oder einfügen
    jsonLabel: Kontakt-JSON
    jsonPlaceholder: Exportiertes Identitäts-JSON hier einfügen
    preview: Vorschau
    includeAvatar: Profilbild übernehmen
    selectClaims: Claims zum Importieren auswählen
  manual:
    labelPlaceholder: z.B. Alice, Bob, Team-Lead
    publicKeyPlaceholder: Base58-kodierten Public Key einfügen
    notesPlaceholder: Optionale Notizen
  scan:
    selectCamera: Kamera auswählen
    refreshCameras: Kameras neu laden
    cameraError: Kamera konnte nicht gestartet werden. Bitte erlaube den Kamerazugriff.
    invalidQr: Ungültiger QR-Code. Bitte scanne einen Identitäts-QR-Code.
    reviewLabel: Name
    noClaims: Keine Claims vorhanden. Du kannst eigene hinzufügen.
    alreadyExists: 'Ein Kontakt mit diesem Public Key existiert bereits: {name}'
    alreadyExistsOwn: 'Das ist deine eigene Identität ({name}) und kann nicht als Kontakt hinzugefügt werden'
  fields:
    label: Name
    publicKey: Public Key
    notes: Notizen
  claims:
    title: Claims
    add: Hinzufügen
    type: Typ
    value: Wert
  actions:
    add: Hinzufügen
    cancel: Abbrechen
    back: Zurück
  success:
    added: Kontakt hinzugefügt
  errors:
    addFailed: Kontakt konnte nicht hinzugefügt werden
    importFailed: Import fehlgeschlagen
    invalidJson: Ungültiges JSON-Format
    invalidData: Unvollständige Daten (did erforderlich)
en:
  title: Add Contact
  description: Scan a QR code, import from a file or add manually
  tabs:
    scan: QR Code
    file: From file
    manual: Manual
  file:
    selectFile: Select JSON file
    orPaste: or paste
    jsonLabel: Contact JSON
    jsonPlaceholder: Paste exported identity JSON here
    preview: Preview
    includeAvatar: Include profile picture
    selectClaims: Select claims to import
  manual:
    labelPlaceholder: e.g. Alice, Bob, Team Lead
    publicKeyPlaceholder: Paste Base58-encoded public key
    notesPlaceholder: Optional notes
  scan:
    selectCamera: Select camera
    refreshCameras: Refresh cameras
    cameraError: Could not start camera. Please allow camera access.
    invalidQr: Invalid QR code. Please scan an identity QR code.
    reviewLabel: Name
    noClaims: No claims yet. You can add your own.
    alreadyExists: 'A contact with this public key already exists: {name}'
    alreadyExistsOwn: 'This is your own identity ({name}) and cannot be added as a contact'
  fields:
    label: Name
    publicKey: Public Key
    notes: Notes
  claims:
    title: Claims
    add: Add
    type: Type
    value: Value
  actions:
    add: Add
    cancel: Cancel
    back: Back
  success:
    added: Contact added
  errors:
    addFailed: Failed to add contact
    importFailed: Failed to import file
    invalidJson: Invalid JSON format
    invalidData: Incomplete data (did is required)
</i18n>
