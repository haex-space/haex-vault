import type { Ref } from 'vue'
import { didKeyToPublicKeyAsync } from '@haex-space/vault-sdk'
import type { SelectHaexIdentities } from '~/database/schemas'
import { createLogger } from '@/stores/logging'

const log = createLogger('CONTACTS:ADD')

export interface ScannedClaim {
  type: string
  value: string
  selected: boolean
}

export interface ScannedContact {
  did: string
  endpointId?: string
  name: string
  claims: ScannedClaim[]
}

export interface ImportParsed {
  name: string
  did: string
  avatar?: string | null
  claims: { type: string; value: string }[]
}

interface UseAddContactWizardOptions {
  open: Ref<boolean>
  onAdded: () => void
}

export function useAddContactWizard(options: UseAddContactWizardOptions) {
  const { open, onAdded } = options
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
  const importParsed = ref<ImportParsed | null>(null)
  const importSelectedClaimIndices = ref(new Set<number>())
  const importIncludeAvatar = ref(true)

  // --- Manual state ---
  const manualForm = reactive({
    label: '',
    publicKey: '',
    notes: '',
  })

  // --- Scan state ---
  const scanStep = ref<'scan' | 'review'>('scan')
  const scanError = ref('')
  const scannedContact = ref<ScannedContact | null>(null)
  const scanContactNotes = ref('')
  const scanIsSaving = ref(false)
  const scanExistingContact = ref<SelectHaexIdentities | null>(null)
  const scanBlockingIdentity = computed(() => {
    const existing = scanExistingContact.value
    if (!existing) return null
    return existing.source === 'contact' || existing.source === 'own' ? existing : null
  })
  const scanShowAddClaimInline = ref(false)
  const scanNewClaimType = ref('')
  const scanNewClaimValue = ref('')

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
  }

  // --- Footer ---
  const backLabel = computed(() => {
    if (addMode.value === 'file' && importParsed.value) return t('actions.back')
    if (addMode.value === 'scan' && scanStep.value === 'review') return t('actions.back')
    return t('actions.cancel')
  })

  // --- Manual add ---
  const onAddManualContactAsync = async () => {
    if (!manualForm.label.trim() || !manualForm.publicKey.trim()) return

    log.info(`Adding contact manually: "${manualForm.label}"`)
    isAdding.value = true
    try {
      await identityStore.addContactAsync({
        name: manualForm.label.trim(),
        publicKey: manualForm.publicKey.trim(),
        notes: manualForm.notes.trim() || undefined,
      })
      log.info('Manual contact added successfully')
      addToast({ title: t('success.added'), color: 'success' })
      open.value = false
      onAdded()
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

    const did = typeof parsed.did === 'string' ? parsed.did : undefined

    if (!did) {
      log.warn('Import data missing did')
      addToast({ title: t('errors.invalidData'), color: 'error' })
      return
    }

    const claims = Array.isArray(parsed.claims)
      ? (parsed.claims as { type: string; value: string }[])
      : []

    importParsed.value = {
      name: (parsed.name as string) || '',
      did,
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

      // The store's contact APIs key off publicKey, so derive it from the DID.
      const publicKey = await didKeyToPublicKeyAsync(data.did)
      const displayName = data.name || `Imported ${data.did.slice(0, 16)}...`

      log.info(`Importing contact: "${displayName}", ${selectedClaims.length}/${data.claims.length} claims, avatar: ${!!avatar}`)
      const contact = await identityStore.addContactWithClaimsAsync({
        name: displayName,
        publicKey,
        claims: selectedClaims,
        avatar,
      })

      log.info(`Contact imported successfully (id: ${contact.id})`)
      addToast({ title: t('success.added'), color: 'success' })
      open.value = false
      onAdded()
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

  // --- Scan save ---
  const onSaveScanContactAsync = async () => {
    if (!scannedContact.value || !scannedContact.value.name.trim()) return

    scanIsSaving.value = true
    try {
      const selectedClaims = scannedContact.value.claims
        .filter(c => c.selected)
        .map(c => ({ type: c.type, value: c.value }))

      const endpointValue = scannedContact.value.endpointId
      if (endpointValue && !selectedClaims.some(c => c.value === endpointValue)) {
        selectedClaims.push({ type: 'endpointId', value: endpointValue })
      }

      log.info(`Saving scanned contact: "${scannedContact.value.name}", ${selectedClaims.length} claims`)
      await identityStore.addContactWithClaimsAsync({
        name: scannedContact.value.name.trim(),
        publicKey: await didKeyToPublicKeyAsync(scannedContact.value.did),
        claims: selectedClaims,
        notes: scanContactNotes.value.trim() || undefined,
      })

      log.info('Scanned contact saved successfully')
      addToast({ title: t('success.added'), color: 'success' })
      open.value = false
      onAdded()
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

  // Used by ModeScan when a QR is decoded
  const ingestScannedPayload = async (payload: { did?: string; endpointId?: string; name?: string; claims?: { type: string; value: string }[] }) => {
    if (!payload.did) return false

    const existing = await identityStore.getIdentityByDidAsync(payload.did)
    scanExistingContact.value = existing ?? null

    if (existing) {
      log.info(`Scanned contact already exists: ${existing.name} (${existing.id})`)
    }

    scannedContact.value = {
      did: payload.did,
      endpointId: payload.endpointId || undefined,
      name: payload.name || '',
      claims: (payload.claims || []).map(c => ({
        type: c.type,
        value: c.value,
        selected: true,
      })),
    }

    log.info(`Scanned contact: "${payload.name || '(no name)'}", ${payload.claims?.length ?? 0} claims, endpointId: ${!!payload.endpointId}`)
    scanContactNotes.value = ''
    scanStep.value = 'review'
    return true
  }

  return {
    // shared
    isAdding,
    addMode,
    addTabItems,
    backLabel,
    // file
    importJson,
    importParsed,
    importSelectedClaimIndices,
    importIncludeAvatar,
    resetFileImport,
    onSelectImportFileAsync,
    toggleImportClaim,
    onParseImport,
    onImportContactAsync,
    // manual
    manualForm,
    resetManualForm,
    onAddManualContactAsync,
    // scan
    scanStep,
    scanError,
    scannedContact,
    scanContactNotes,
    scanIsSaving,
    scanExistingContact,
    scanBlockingIdentity,
    scanShowAddClaimInline,
    scanNewClaimType,
    scanNewClaimValue,
    resetScanState,
    onSaveScanContactAsync,
    addScanInlineClaim,
    ingestScannedPayload,
  }
}
