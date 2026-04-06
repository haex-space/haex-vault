<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <UTabs
        v-model="addMode"
        :items="addTabItems"
        class="w-full"
      />

      <!-- File import mode -->
      <template v-if="addMode === 'file'">
        <!-- Step 1: Select file or paste JSON -->
        <template v-if="!importParsed">
          <div class="space-y-4 mt-4">
            <UButton
              color="neutral"
              variant="outline"
              icon="i-lucide-file-up"
              block
              @click="onSelectImportFileAsync"
            >
              {{ t('file.selectFile') }}
            </UButton>

            <USeparator :label="t('file.orPaste')" />

            <UiTextarea
              v-model="importJson"
              :label="t('file.jsonLabel')"
              :placeholder="t('file.jsonPlaceholder')"
              :rows="6"
              data-testid="contacts-import-json"
            />
          </div>
        </template>

        <!-- Step 2: Preview & select -->
        <template v-else>
          <div class="space-y-4 mt-4">
            <div class="flex items-center gap-3 p-3 rounded-lg border border-default">
              <UiAvatar
                v-if="importParsed.avatar"
                :src="importParsed.avatar"
                :seed="importParsed.publicKey"
                size="sm"
              />
              <div class="min-w-0 flex-1">
                <p class="font-medium truncate">{{ importParsed.label || importParsed.publicKey.slice(0, 20) + '...' }}</p>
                <p class="text-xs text-muted truncate">{{ importParsed.publicKey }}</p>
              </div>
            </div>

            <div
              v-if="importParsed.avatar"
              class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <UCheckbox v-model="importIncludeAvatar" />
              <UiAvatar
                :src="importParsed.avatar"
                :seed="importParsed.publicKey"
                size="sm"
              />
              <span class="text-sm">{{ t('file.includeAvatar') }}</span>
            </div>

            <div
              v-if="importParsed.claims.length"
              class="space-y-2"
            >
              <span class="text-sm font-medium">{{ t('file.selectClaims') }}</span>
              <div
                v-for="(claim, index) in importParsed.claims"
                :key="index"
                class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
              >
                <UCheckbox
                  :model-value="importSelectedClaimIndices.has(index)"
                  @update:model-value="toggleImportClaim(index)"
                />
                <div class="min-w-0 flex-1">
                  <span class="text-xs font-medium text-muted">{{ claim.type }}</span>
                  <p class="text-sm truncate">{{ claim.value }}</p>
                </div>
              </div>
            </div>
          </div>
        </template>
      </template>

      <!-- Manual mode -->
      <template v-else-if="addMode === 'manual'">
        <div class="space-y-4 mt-4">
          <UiInput
            v-model="manualForm.label"
            :label="t('fields.label')"
            :placeholder="t('manual.labelPlaceholder')"
          />
          <UiTextarea
            v-model="manualForm.publicKey"
            :label="t('fields.publicKey')"
            :placeholder="t('manual.publicKeyPlaceholder')"
            :rows="3"
          />
          <UiTextarea
            v-model="manualForm.notes"
            :label="t('fields.notes')"
            :placeholder="t('manual.notesPlaceholder')"
            :rows="2"
          />
        </div>
      </template>

      <!-- Scan QR mode -->
      <template v-else-if="addMode === 'scan'">
        <!-- Step 1: Scan QR code -->
        <template v-if="scanStep === 'scan'">
          <div class="space-y-3 mt-4">
            <USelectMenu
              v-if="scanCameras.length > 1"
              v-model="scanSelectedCameraId"
              :items="scanCameraOptions"
              value-key="value"
              :placeholder="t('scan.selectCamera')"
              class="w-full"
            />
            <div
              ref="scannerContainer"
              class="w-full rounded-lg overflow-hidden"
            />
            <p
              v-if="scanError"
              class="text-sm text-red-500"
            >
              {{ scanError }}
            </p>
          </div>
        </template>

        <!-- Step 2: Review scanned contact -->
        <template v-if="scanStep === 'review' && scannedContact">
          <div class="space-y-4 mt-4">
            <UiInput
              v-model="scannedContact.label"
              :label="t('scan.reviewLabel')"
            />

            <div>
              <label class="text-sm font-medium">{{ t('fields.publicKey') }}</label>
              <code
                class="block text-xs text-muted p-2 rounded bg-gray-50 dark:bg-gray-800/50 break-all mt-1"
              >
                {{ scannedContact.publicKey }}
              </code>
            </div>

            <div class="space-y-2">
              <div class="flex items-center justify-between">
                <span class="text-sm font-medium">{{ t('claims.title') }}</span>
                <UiButton
                  variant="outline"
                  icon="i-lucide-plus"
                  @click="scanShowAddClaimInline = true"
                >
                  {{ t('claims.add') }}
                </UiButton>
              </div>

              <div
                v-for="(claim, index) in scannedContact.claims"
                :key="index"
                class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
              >
                <UiToggle v-model="claim.selected" />
                <div class="min-w-0 flex-1">
                  <span class="text-xs font-medium text-muted">{{ claim.type }}</span>
                  <UiInput
                    v-model="claim.value"
                    class="mt-1"
                  />
                </div>
                <UiButton
                  variant="ghost"
                  color="error"
                  icon="i-lucide-x"
                  @click="scannedContact.claims.splice(index, 1)"
                />
              </div>

              <!-- Inline add claim form -->
              <div
                v-if="scanShowAddClaimInline"
                class="flex items-end gap-2 p-2 rounded border border-dashed border-default"
              >
                <UiInput
                  v-model="scanNewClaimType"
                  :label="t('claims.type')"
                  placeholder="email, phone, ..."
                  class="flex-1"
                />
                <UiInput
                  v-model="scanNewClaimValue"
                  :label="t('claims.value')"
                  class="flex-1"
                  @keydown.enter.prevent="addScanInlineClaim"
                />
                <UiButton
                  icon="i-lucide-check"
                  :disabled="!scanNewClaimType.trim() || !scanNewClaimValue.trim()"
                  @click="addScanInlineClaim"
                />
                <UiButton
                  variant="ghost"
                  icon="i-lucide-x"
                  @click="scanShowAddClaimInline = false"
                />
              </div>

              <p
                v-if="!scannedContact.claims.length && !scanShowAddClaimInline"
                class="text-xs text-muted"
              >
                {{ t('scan.noClaims') }}
              </p>
            </div>

            <UiTextarea
              v-model="scanContactNotes"
              :label="t('fields.notes')"
              :placeholder="t('manual.notesPlaceholder')"
              :rows="2"
            />

            <p
              v-if="scanExistingContact"
              class="text-sm text-amber-500"
            >
              {{ t('scan.alreadyExists', { name: scanExistingContact.label }) }}
            </p>
          </div>
        </template>
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <div class="flex gap-2">
          <UButton
            color="neutral"
            variant="outline"
            @click="onBack"
          >
            {{ backLabel }}
          </UButton>
          <UiButton
            v-if="addMode === 'scan' && scanStep === 'scan'"
            icon="i-lucide-refresh-cw"
            color="neutral"
            variant="outline"
            :title="t('scan.refreshCameras')"
            @click="refreshScanCameras"
          />
        </div>

        <!-- File mode buttons -->
        <template v-if="addMode === 'file'">
          <UiButton
            v-if="!importParsed"
            icon="i-lucide-arrow-right"
            :disabled="!importJson.trim()"
            data-testid="contacts-import-preview"
            @click="onParseImport"
          >
            {{ t('file.preview') }}
          </UiButton>
          <UiButton
            v-else
            icon="i-lucide-plus"
            :loading="isAdding"
            data-testid="contacts-import-submit"
            @click="onImportContactAsync"
          >
            {{ t('actions.add') }}
          </UiButton>
        </template>

        <!-- Manual mode button -->
        <UiButton
          v-else-if="addMode === 'manual'"
          icon="i-lucide-plus"
          :loading="isAdding"
          :disabled="!manualForm.label.trim() || !manualForm.publicKey.trim()"
          @click="onAddManualContactAsync"
        >
          {{ t('actions.add') }}
        </UiButton>

        <!-- Scan mode: save button (only in review step) -->
        <UiButton
          v-else-if="addMode === 'scan' && scanStep === 'review'"
          icon="i-lucide-user-plus"
          :loading="scanIsSaving"
          :disabled="!scannedContact?.label.trim() || !!scanExistingContact"
          @click="onSaveScanContactAsync"
        >
          {{ t('actions.add') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { Html5Qrcode } from 'html5-qrcode'
import type { SelectHaexIdentities } from '~/database/schemas'
import { createLogger } from '@/stores/logging'

const log = createLogger('CONTACTS:ADD')

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
  added: []
}>()

const { t } = useI18n()
const { add: addToast } = useToast()
const identityStore = useIdentityStore()

const isAdding = ref(false)
const addMode = ref<string>('file')

const addTabItems = computed(() => [
  { label: t('tabs.scan'), value: 'scan' },
  { label: t('tabs.file'), value: 'file' },
  { label: t('tabs.manual'), value: 'manual' },
])

// --- File import state ---
const importJson = ref('')
const importParsed = ref<{
  label: string
  publicKey: string
  avatar?: string | null
  claims: { type: string; value: string }[]
} | null>(null)
const importSelectedClaimIndices = ref(new Set<number>())
const importIncludeAvatar = ref(true)

// --- Manual state ---
const manualForm = reactive({
  label: '',
  publicKey: '',
  notes: '',
})

// --- QR scan state ---
const scanStep = ref<'scan' | 'review'>('scan')
const scannerContainer = ref<HTMLElement | null>(null)
const scanError = ref('')
const scannedContact = ref<ScannedContact | null>(null)
const scanContactNotes = ref('')
const scanIsSaving = ref(false)
const scanExistingContact = ref<SelectHaexIdentities | null>(null)
const scanShowAddClaimInline = ref(false)
const scanNewClaimType = ref('')
const scanNewClaimValue = ref('')
const scanCameras = ref<{ id: string; label: string }[]>([])
const scanSelectedCameraId = ref('')
let qrScanner: Html5Qrcode | null = null

const scanCameraOptions = computed(() =>
  scanCameras.value.map(c => ({
    label: c.label || c.id,
    value: c.id,
  })),
)

// --- Footer ---
const backLabel = computed(() => {
  if (addMode.value === 'file' && importParsed.value) return t('actions.back')
  if (addMode.value === 'scan' && scanStep.value === 'review') return t('actions.back')
  return t('actions.cancel')
})

const onBack = () => {
  if (addMode.value === 'file' && importParsed.value) {
    importParsed.value = null
  } else if (addMode.value === 'scan' && scanStep.value === 'review') {
    backToScan()
  } else {
    open.value = false
  }
}

// --- Dialog open/close ---
watch(open, async (isOpen) => {
  if (isOpen) {
    addMode.value = ''
    resetFileImport()
    resetManualForm()
    resetScanState()
    addMode.value = 'scan'
  } else {
    await stopQrScanner()
  }
})

watch(addMode, async (newMode, oldMode) => {
  if (oldMode === 'scan') await stopQrScanner()
  if (newMode === 'scan' && open.value) {
    log.info(`Mode changed to scan`)
    resetScanState()
    await nextTick()
    await loadScanCameras()
    startQrScanner()
  }
})

watch(scanSelectedCameraId, async (newId, oldId) => {
  if (newId && oldId && newId !== oldId) {
    await stopQrScanner()
    await nextTick()
    startQrScanner()
  }
})

// --- Reset helpers ---
const resetFileImport = () => {
  importJson.value = ''
  importParsed.value = null
  importSelectedClaimIndices.value.clear()
  importIncludeAvatar.value = true
}

const resetManualForm = () => {
  manualForm.label = ''
  manualForm.publicKey = ''
  manualForm.notes = ''
}

const resetScanState = () => {
  scanStep.value = 'scan'
  scanError.value = ''
  scannedContact.value = null
  scanContactNotes.value = ''
  scanExistingContact.value = null
  scanShowAddClaimInline.value = false
  scanNewClaimType.value = ''
  scanNewClaimValue.value = ''
  scanCameras.value = []
  scanSelectedCameraId.value = ''
}

// --- QR scanner ---
const loadScanCameras = async () => {
  try {
    const devices = await Html5Qrcode.getCameras()
    scanCameras.value = devices.map(d => ({ id: d.id, label: d.label }))
    log.info(`Found ${devices.length} camera(s)`, scanCameras.value.map(c => c.label))
    if (scanCameras.value.length > 0 && !scanCameras.value.some(c => c.id === scanSelectedCameraId.value)) {
      scanSelectedCameraId.value = scanCameras.value[0]?.id ?? ''
    }
  } catch (error) {
    log.error('Failed to enumerate cameras', error)
    scanError.value = t('scan.cameraError')
  }
}

const startQrScanner = async () => {
  if (!scannerContainer.value) return

  const containerId = 'qr-scanner-' + Date.now()
  scannerContainer.value.id = containerId
  const cameraId = scanSelectedCameraId.value || 'environment'
  log.info(`Starting QR scanner (camera: ${cameraId})`)

  try {
    qrScanner = new Html5Qrcode(containerId)
    await qrScanner.start(
      scanSelectedCameraId.value || { facingMode: 'environment' },
      { fps: 10, qrbox: { width: 250, height: 250 } },
      onQrScanSuccess,
      undefined,
    )
    log.info('QR scanner started successfully')
  } catch (error) {
    log.error('Failed to start QR scanner', error)
    scanError.value = t('scan.cameraError')
  }
}

const stopQrScanner = async () => {
  if (qrScanner) {
    try {
      if (qrScanner.isScanning) {
        await qrScanner.stop()
      }
    } catch {
      // Scanner might already be stopped
    }
    qrScanner = null
  }
  if (scannerContainer.value) {
    scannerContainer.value.replaceChildren()
  }
}

const refreshScanCameras = async () => {
  await stopQrScanner()
  await loadScanCameras()
  await nextTick()
  startQrScanner()
}

const onQrScanSuccess = async (decodedText: string) => {
  log.info('QR code decoded, processing payload')
  await stopQrScanner()

  try {
    const payload = JSON.parse(decodedText)

    if (!payload.publicKey) {
      log.warn('QR payload missing publicKey, restarting scanner')
      scanError.value = t('scan.invalidQr')
      scanStep.value = 'scan'
      await nextTick()
      startQrScanner()
      return
    }

    const existing = await identityStore.getContactByPublicKeyAsync(payload.publicKey)
    scanExistingContact.value = existing ?? null

    if (existing) {
      log.info(`Scanned contact already exists: ${existing.label} (${existing.id})`)
    }

    scannedContact.value = {
      publicKey: payload.publicKey,
      endpointId: payload.endpointId || undefined,
      label: payload.label || '',
      claims: (payload.claims || []).map((c: { type: string; value: string }) => ({
        type: c.type,
        value: c.value,
        selected: true,
      })),
    }

    log.info(`Scanned contact: "${payload.label || '(no label)'}", ${payload.claims?.length ?? 0} claims, endpointId: ${!!payload.endpointId}`)
    scanContactNotes.value = ''
    scanStep.value = 'review'
  } catch (error) {
    log.warn('QR payload is not valid JSON, restarting scanner', error)
    scanError.value = t('scan.invalidQr')
    await nextTick()
    startQrScanner()
  }
}

const backToScan = async () => {
  scanStep.value = 'scan'
  scannedContact.value = null
  scanExistingContact.value = null
  scanError.value = ''
  await nextTick()
  startQrScanner()
}

const onSaveScanContactAsync = async () => {
  if (!scannedContact.value || !scannedContact.value.label.trim()) return

  scanIsSaving.value = true
  try {
    const selectedClaims = scannedContact.value.claims
      .filter(c => c.selected)
      .map(c => ({ type: c.type, value: c.value }))

    if (scannedContact.value.endpointId) {
      selectedClaims.push({ type: 'endpointId', value: scannedContact.value.endpointId })
    }

    log.info(`Saving scanned contact: "${scannedContact.value.label}", ${selectedClaims.length} claims`)
    await identityStore.addContactWithClaimsAsync(
      scannedContact.value.label.trim(),
      scannedContact.value.publicKey,
      selectedClaims,
      scanContactNotes.value.trim() || undefined,
    )

    log.info('Scanned contact saved successfully')
    addToast({ title: t('success.added'), color: 'success' })
    open.value = false
    emit('added')
  } catch (error) {
    log.error('Failed to save scanned contact', error)
    addToast({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    scanIsSaving.value = false
  }
}

const addScanInlineClaim = () => {
  if (!scannedContact.value || !scanNewClaimType.value.trim() || !scanNewClaimValue.value.trim()) return
  scannedContact.value.claims.push({
    type: scanNewClaimType.value.trim(),
    value: scanNewClaimValue.value.trim(),
    selected: true,
  })
  scanNewClaimType.value = ''
  scanNewClaimValue.value = ''
  scanShowAddClaimInline.value = false
}

// --- Manual add ---
const onAddManualContactAsync = async () => {
  if (!manualForm.label.trim() || !manualForm.publicKey.trim()) return

  log.info(`Adding contact manually: "${manualForm.label}"`)
  isAdding.value = true
  try {
    await identityStore.addContactAsync(
      manualForm.label.trim(),
      manualForm.publicKey.trim(),
      manualForm.notes.trim() || undefined,
    )
    log.info('Manual contact added successfully')
    addToast({ title: t('success.added'), color: 'success' })
    open.value = false
    emit('added')
  } catch (error) {
    log.error('Failed to add manual contact', error)
    addToast({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isAdding.value = false
  }
}

// --- File import ---
const onSelectImportFileAsync = async () => {
  try {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
    const { readFile } = await import('@tauri-apps/plugin-fs')

    const filePath = await openDialog({
      title: t('title'),
      filters: [{ name: 'JSON', extensions: ['json'] }],
      multiple: false,
    })
    if (!filePath) return

    log.info(`Reading contact file: ${filePath}`)
    const data = await readFile(filePath as string)
    importJson.value = new TextDecoder().decode(data)
    log.info(`File loaded (${data.byteLength} bytes)`)
  } catch (error) {
    log.error('Failed to read import file', error)
    addToast({
      title: t('errors.importFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const toggleImportClaim = (index: number) => {
  if (importSelectedClaimIndices.value.has(index)) {
    importSelectedClaimIndices.value.delete(index)
  } else {
    importSelectedClaimIndices.value.add(index)
  }
}

const onParseImport = () => {
  if (!importJson.value.trim()) return

  let parsed: Record<string, unknown>
  try {
    parsed = JSON.parse(importJson.value)
  } catch {
    log.warn('Import JSON parse failed')
    addToast({ title: t('errors.invalidJson'), color: 'error' })
    return
  }

  if (!parsed.publicKey) {
    log.warn('Import data missing publicKey')
    addToast({ title: t('errors.invalidData'), color: 'error' })
    return
  }

  const claims = Array.isArray(parsed.claims)
    ? (parsed.claims as { type: string; value: string }[])
    : []

  importParsed.value = {
    label: (parsed.label as string) || '',
    publicKey: parsed.publicKey as string,
    avatar: typeof parsed.avatar === 'string' ? parsed.avatar : null,
    claims,
  }

  importSelectedClaimIndices.value = new Set(claims.map((_, i) => i))
  importIncludeAvatar.value = !!importParsed.value.avatar
}

const onImportContactAsync = async () => {
  if (!importParsed.value) return

  isAdding.value = true
  try {
    const data = importParsed.value
    const selectedClaims = data.claims.filter((_, i) => importSelectedClaimIndices.value.has(i))
    const avatar = importIncludeAvatar.value ? data.avatar : null

    log.info(`Importing contact: "${data.label}", ${selectedClaims.length}/${data.claims.length} claims, avatar: ${!!avatar}`)
    const contact = await identityStore.addContactWithClaimsAsync(
      data.label || `Imported ${data.publicKey.slice(0, 16)}...`,
      data.publicKey,
      selectedClaims,
    )
    if (avatar) {
      await identityStore.updateContactAsync(contact.id, { avatar })
    }

    log.info(`Contact imported successfully (id: ${contact.id})`)
    addToast({ title: t('success.added'), color: 'success' })
    open.value = false
    emit('added')
  } catch (error) {
    log.error('Failed to import contact', error)
    addToast({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isAdding.value = false
  }
}

onBeforeUnmount(() => {
  stopQrScanner()
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
    invalidData: Unvollständige Daten (mindestens publicKey erforderlich)
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
    invalidData: Incomplete data (at least publicKey is required)
</i18n>
