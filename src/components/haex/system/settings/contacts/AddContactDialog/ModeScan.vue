<template>
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
        v-model="scannedContact.name"
        :label="t('scan.reviewLabel')"
      />

      <div>
        <label class="text-sm font-medium">DID</label>
        <code
          class="block text-xs text-muted p-2 rounded bg-gray-50 dark:bg-gray-800/50 break-all mt-1"
        >
          {{ scannedContact.did }}
        </code>
      </div>

      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">{{ t('claims.title') }}</span>
          <UiButton
            variant="outline"
            icon="i-lucide-plus"
            @click="() => { scanShowAddClaimInlineProxy = true }"
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
            @click="() => { scannedContact?.claims.splice(index, 1) }"
          />
        </div>

        <!-- Inline add claim form -->
        <div
          v-if="scanShowAddClaimInline"
          class="flex items-end gap-2 p-2 rounded border border-dashed border-default"
        >
          <UiInput
            v-model="scanNewClaimTypeProxy"
            :label="t('claims.type')"
            placeholder="email, phone, ..."
            class="flex-1"
          />
          <UiInput
            v-model="scanNewClaimValueProxy"
            :label="t('claims.value')"
            class="flex-1"
            @keydown.enter.prevent="onAddInlineClaim"
          />
          <UiButton
            icon="i-lucide-check"
            :disabled="!scanNewClaimType.trim() || !scanNewClaimValue.trim()"
            @click="onAddInlineClaim"
          />
          <UiButton
            variant="ghost"
            icon="i-lucide-x"
            @click="() => { scanShowAddClaimInlineProxy = false }"
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
        v-model="scanContactNotesProxy"
        :label="t('fields.notes')"
        :placeholder="t('manual.notesPlaceholder')"
        :rows="2"
      />

      <p
        v-if="scanBlockingIdentity"
        class="text-sm text-amber-500"
      >
        {{ t(
          scanBlockingIdentity.source === 'own' ? 'scan.alreadyExistsOwn' : 'scan.alreadyExists',
          { name: scanBlockingIdentity.name },
        ) }}
      </p>
    </div>
  </template>
</template>

<script setup lang="ts">
import { Html5Qrcode } from 'html5-qrcode'
import type { SelectHaexIdentities } from '~/database/schemas'
import type { ScannedContact } from '@/composables/contacts/useAddContactWizard'
import { createLogger } from '@/stores/logging'

const log = createLogger('CONTACTS:ADD:SCAN')

const props = defineProps<{
  active: boolean
  scanStep: 'scan' | 'review'
  scannedContact: ScannedContact | null
  scanContactNotes: string
  scanError: string
  scanBlockingIdentity: SelectHaexIdentities | null
  scanShowAddClaimInline: boolean
  scanNewClaimType: string
  scanNewClaimValue: string
}>()

const emit = defineEmits<{
  'update:scanStep': [value: 'scan' | 'review']
  'update:scanError': [value: string]
  'update:scanContactNotes': [value: string]
  'update:scanShowAddClaimInline': [value: boolean]
  'update:scanNewClaimType': [value: string]
  'update:scanNewClaimValue': [value: string]
  ingest: [payload: { did?: string; endpointId?: string; name?: string; claims?: { type: string; value: string }[] }]
  addInlineClaim: []
}>()

const { t } = useI18n()

// Two-way proxies for scalar refs in the composable
const scanContactNotesProxy = computed({
  get: () => props.scanContactNotes,
  set: v => emit('update:scanContactNotes', v),
})
const scanShowAddClaimInlineProxy = computed({
  get: () => props.scanShowAddClaimInline,
  set: v => emit('update:scanShowAddClaimInline', v),
})
const scanNewClaimTypeProxy = computed({
  get: () => props.scanNewClaimType,
  set: v => emit('update:scanNewClaimType', v),
})
const scanNewClaimValueProxy = computed({
  get: () => props.scanNewClaimValue,
  set: v => emit('update:scanNewClaimValue', v),
})

// Local scanner state — kept here because it is purely a view-layer concern
const scannerContainer = ref<HTMLElement | null>(null)
const scanCameras = ref<{ id: string; label: string }[]>([])
const scanSelectedCameraId = ref('')
let qrScanner: Html5Qrcode | null = null

const scanCameraOptions = computed(() =>
  scanCameras.value.map(c => ({
    label: c.label || c.id,
    value: c.id,
  })),
)

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
    emit('update:scanError', t('scan.cameraError'))
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
    emit('update:scanError', t('scan.cameraError'))
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

    if (!payload.did) {
      log.warn('QR payload missing did, restarting scanner')
      emit('update:scanError', t('scan.invalidQr'))
      emit('update:scanStep', 'scan')
      await nextTick()
      startQrScanner()
      return
    }

    emit('ingest', payload)
  } catch (error) {
    log.warn('QR payload is not valid JSON, restarting scanner', error)
    emit('update:scanError', t('scan.invalidQr'))
    await nextTick()
    startQrScanner()
  }
}

const onAddInlineClaim = () => emit('addInlineClaim')

// With parent v-if mounting, the scanner lifecycle is bound to component
// mount/unmount. The `active` prop exists for the parent to suspend us
// without unmount if it ever wants to (currently always true while mounted).
const initScanner = async () => {
  log.info(`Mode changed to scan`)
  await nextTick()
  await loadScanCameras()
  startQrScanner()
}

onMounted(() => {
  if (props.active) initScanner()
})

// Restart scanner when switching back to scan sub-step (after review back)
watch(() => props.scanStep, async (step) => {
  if (step === 'scan' && props.active) {
    await nextTick()
    if (!qrScanner) startQrScanner()
  }
})

watch(scanSelectedCameraId, async (newId, oldId) => {
  if (newId && oldId && newId !== oldId) {
    await stopQrScanner()
    await nextTick()
    startQrScanner()
  }
})

onBeforeUnmount(() => {
  stopQrScanner()
})

defineExpose({
  refreshScanCameras,
  startQrScanner,
  stopQrScanner,
})
</script>
