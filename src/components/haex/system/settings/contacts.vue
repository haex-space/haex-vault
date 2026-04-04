<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
  >
    <template #actions>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-share-2"
        @click="showShareDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.share') }}</span>
      </UButton>
      <UButton
        color="primary"
        icon="i-lucide-plus"
        @click="showAddDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.add') }}</span>
      </UButton>
    </template>

    <!-- Loading -->
    <div
      v-if="isLoading"
      class="flex items-center justify-center py-8"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-5 h-5 animate-spin text-primary"
      />
    </div>

    <!-- Contacts list -->
    <div
      v-else-if="contacts.length"
      class="space-y-3"
    >
      <div
        v-for="contact in contacts"
        :key="contact.id"
        class="p-3 rounded-lg border border-default"
      >
        <UCollapsible
          :open="expandedContact === contact.id"
          :unmount-on-hide="false"
          @update:open="(val: boolean) => onToggleContact(contact.id, val)"
        >
          <div class="flex items-center justify-between cursor-pointer">
            <div class="flex-1 min-w-0 flex items-center gap-2">
              <UIcon
                name="i-lucide-chevron-right"
                class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
                :class="{ 'rotate-90': expandedContact === contact.id }"
              />
              <UiAvatar
                :src="contact.avatar"
                :seed="contact.id"
                size="sm"
              />
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-medium truncate">{{ contact.label }}</span>
                </div>
                <div class="mt-1 flex items-center gap-2">
                  <code class="text-xs text-muted truncate max-w-[300px]">{{
                    contact.publicKey
                  }}</code>
                </div>
              </div>
            </div>

            <div
              class="shrink-0 ml-4"
              @click.stop
            >
              <!-- Large screens: inline buttons -->
              <div class="hidden @md:flex items-center gap-1">
                <UButton
                  variant="ghost"
                  icon="i-lucide-copy"
                  :title="t('actions.copyKey')"
                  @click="copyPublicKey(contact.publicKey)"
                />
                <UButton
                  variant="ghost"
                  icon="i-lucide-pencil"
                  :title="t('actions.edit')"
                  @click="openEditDialog(contact)"
                />
                <UButton
                  variant="ghost"
                  color="error"
                  icon="i-lucide-trash-2"
                  :title="t('actions.delete')"
                  @click="prepareDelete(contact)"
                />
              </div>
              <!-- Small screens: dropdown menu -->
              <UDropdownMenu
                class="@md:hidden"
                :items="[
                  [
                    {
                      label: t('actions.copyKey'),
                      icon: 'i-lucide-copy',
                      onSelect: () => copyPublicKey(contact.publicKey),
                    },
                    {
                      label: t('actions.edit'),
                      icon: 'i-lucide-pencil',
                      onSelect: () => openEditDialog(contact),
                    },
                  ],
                  [
                    {
                      label: t('actions.delete'),
                      icon: 'i-lucide-trash-2',
                      color: 'error' as const,
                      onSelect: () => prepareDelete(contact),
                    },
                  ],
                ]"
              >
                <UButton
                  variant="ghost"
                  icon="i-lucide-ellipsis-vertical"
                  color="neutral"
                />
              </UDropdownMenu>
            </div>
          </div>

          <!-- Claims Section (collapsible) -->
          <template
            v-if="expandedContact === contact.id"
            #content
          >
            <div class="mt-3 pt-3 border-t border-default space-y-2">
              <div class="flex items-center justify-between">
                <span class="text-sm font-medium">{{ t('claims.title') }}</span>
                <UButton
                  variant="outline"
                  icon="i-lucide-plus"
                  @click="openAddClaim(contact.id)"
                >
                  {{ t('claims.add') }}
                </UButton>
              </div>

              <div
                v-if="contactClaims[contact.id]?.length"
                class="space-y-1"
              >
                <div
                  v-for="claim in contactClaims[contact.id]"
                  :key="claim.id"
                  class="flex items-center justify-between p-2 rounded bg-gray-50 dark:bg-gray-800/50"
                >
                  <div class="min-w-0 flex-1">
                    <span class="text-xs font-medium text-muted">{{
                      claim.type
                    }}</span>
                    <p class="text-sm truncate">{{ claim.value }}</p>
                  </div>
                  <div class="flex gap-1 shrink-0 ml-2">
                    <UButton
                      variant="ghost"
                      icon="i-lucide-copy"
                      @click="copyClaimValue(claim.value)"
                    />
                    <UButton
                      variant="ghost"
                      icon="i-lucide-pencil"
                      @click="openEditClaim(claim)"
                    />
                    <UButton
                      variant="ghost"
                      color="error"
                      icon="i-lucide-trash-2"
                      @click="deleteClaimAsync(claim.id, contact.id)"
                    />
                  </div>
                </div>
              </div>
              <p
                v-else
                class="text-xs text-muted"
              >
                {{ t('claims.empty') }}
              </p>

              <!-- Notes -->
              <div
                v-if="contact.notes"
                class="pt-2"
              >
                <span class="text-xs font-medium text-muted">{{
                  t('fields.notes')
                }}</span>
                <p class="text-sm text-muted">{{ contact.notes }}</p>
              </div>
            </div>
          </template>
        </UCollapsible>
      </div>
    </div>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('list.empty')"
      icon="i-lucide-user"
    />
    <!-- Add Contact Dialog -->
    <UiDrawerModal
      v-model:open="showAddDialog"
      :title="t('add.title')"
      :description="t('add.description')"
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
                {{ t('add.selectFile') }}
              </UButton>

              <USeparator :label="t('add.orPaste')" />

              <UiTextarea
                v-model="importJson"
                :label="t('add.jsonLabel')"
                :placeholder="t('add.jsonPlaceholder')"
                :rows="6"
              />
            </div>
          </template>

          <!-- Step 2: Preview & select -->
          <template v-else>
            <div class="space-y-4 mt-4">
              <!-- Contact info -->
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

              <!-- Avatar -->
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
                <span class="text-sm">{{ t('add.includeAvatar') }}</span>
              </div>

              <!-- Claims -->
              <div
                v-if="importParsed.claims.length"
                class="space-y-2"
              >
                <span class="text-sm font-medium">{{ t('add.selectClaims') }}</span>
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
              v-model="addForm.label"
              :label="t('fields.label')"
              :placeholder="t('add.labelPlaceholder')"
            />
            <UiTextarea
              v-model="addForm.publicKey"
              :label="t('fields.publicKey')"
              :placeholder="t('add.publicKeyPlaceholder')"
              :rows="3"
            />
            <UiTextarea
              v-model="addForm.notes"
              :label="t('fields.notes')"
              :placeholder="t('add.notesPlaceholder')"
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
                :placeholder="t('add.notesPlaceholder')"
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
              @click="onAddDialogBack"
            >
              {{ addDialogBackLabel }}
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
              @click="onParseImport"
            >
              {{ t('add.preview') }}
            </UiButton>
            <UiButton
              v-else
              icon="i-lucide-plus"
              :loading="isAdding"
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
            :disabled="!addForm.label.trim() || !addForm.publicKey.trim()"
            @click="onAddContactAsync"
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

    <!-- Edit Contact Dialog -->
    <UiDrawerModal
      v-model:open="showEditDialog"
      :title="t('edit.title')"
    >
      <template #body>
        <UiInput
          v-model="editForm.label"
          :label="t('fields.label')"
        />
        <UiTextarea
          v-model="editForm.notes"
          :label="t('fields.notes')"
          :rows="2"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showEditDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-check"
            :loading="isEditing"
            :disabled="!editForm.label.trim()"
            @click="onEditContactAsync"
          >
            {{ t('actions.save') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Add/Edit Claim Dialog -->
    <UiDrawerModal
      v-model:open="showClaimDialog"
      :title="editingClaim ? t('claims.editTitle') : t('claims.addTitle')"
    >
      <template #body>
        <div class="space-y-4">
          <USelect
            v-if="!editingClaim"
            v-model="claimType"
            class="min-w-48"
            :items="claimTypeOptions"
            value-key="value"
            :label="t('claims.type')"
          />
          <UiInput
            v-if="claimType === 'custom' && !editingClaim"
            v-model="claimCustomType"
            :label="t('claims.customType')"
            placeholder="z.B. phone, company"
          />
          <UiInput
            v-model="claimValue"
            :label="t('claims.value')"
            :placeholder="claimValuePlaceholder"
            @keydown.enter.prevent="onSaveClaimAsync"
          />
        </div>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showClaimDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-check"
            :disabled="!canSaveClaim"
            @click="onSaveClaimAsync"
          >
            {{ t('actions.save') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Delete Confirmation -->
    <UiDialogConfirm
      v-model:open="showDeleteConfirm"
      :title="t('delete.title')"
      :description="t('delete.description')"
      @confirm="onConfirmDeleteAsync"
    />

    <!-- Share Identity QR Dialog -->
    <ShareIdentityDialog v-model:open="showShareDialog" />

  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { Html5Qrcode } from 'html5-qrcode'
import type { SelectHaexIdentities } from '~/database/schemas'
import ShareIdentityDialog from './contacts/ShareIdentityDialog.vue'

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

const { t } = useI18n()
const { add } = useToast()

const identityStore = useIdentityStore()
const { contacts } = storeToRefs(identityStore)

const isLoading = ref(false)
const isAdding = ref(false)
const isEditing = ref(false)

const showAddDialog = ref(false)
const showEditDialog = ref(false)
const showDeleteConfirm = ref(false)
const showShareDialog = ref(false)

const addMode = ref<string>('file')
const addTabItems = computed(() => [
  { label: t('add.tabScan'), value: 'scan' },
  { label: t('add.tabFile'), value: 'file' },
  { label: t('add.tabManual'), value: 'manual' },
])

const importJson = ref('')
const importParsed = ref<{
  label: string
  publicKey: string
  avatar?: string | null
  claims: { type: string; value: string }[]
} | null>(null)
const importSelectedClaimIndices = ref(new Set<number>())
const importIncludeAvatar = ref(true)

const addForm = reactive({
  label: '',
  publicKey: '',
  notes: '',
})

// QR scan state
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

const editForm = reactive({
  id: '',
  label: '',
  notes: '',
})

const deleteTarget = ref<SelectHaexIdentities | null>(null)

onMounted(async () => {
  isLoading.value = true
  try {
    await identityStore.loadIdentitiesAsync()
  } finally {
    isLoading.value = false
  }
})

watch(showAddDialog, async (isOpen) => {
  if (isOpen) {
    // Reset to a non-scan value first so the addMode watcher always triggers
    addMode.value = ''
    importJson.value = ''
    importParsed.value = null
    importSelectedClaimIndices.value.clear()
    importIncludeAvatar.value = true
    addForm.label = ''
    addForm.publicKey = ''
    addForm.notes = ''
    resetScanState()
    // Setting to 'scan' triggers the addMode watcher which starts the scanner
    addMode.value = 'scan'
  } else {
    await stopQrScanner()
  }
})

watch(addMode, async (newMode, oldMode) => {
  if (oldMode === 'scan') await stopQrScanner()
  if (newMode === 'scan' && showAddDialog.value) {
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

// Footer helpers
const addDialogBackLabel = computed(() => {
  if (addMode.value === 'file' && importParsed.value) return t('actions.back')
  if (addMode.value === 'scan' && scanStep.value === 'review') return t('actions.back')
  return t('actions.cancel')
})

const onAddDialogBack = () => {
  if (addMode.value === 'file' && importParsed.value) {
    importParsed.value = null
  } else if (addMode.value === 'scan' && scanStep.value === 'review') {
    backToScan()
  } else {
    showAddDialog.value = false
  }
}

// QR scanner methods
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

const loadScanCameras = async () => {
  try {
    const devices = await Html5Qrcode.getCameras()
    scanCameras.value = devices.map(d => ({ id: d.id, label: d.label }))
    if (scanCameras.value.length > 0 && !scanCameras.value.some(c => c.id === scanSelectedCameraId.value)) {
      scanSelectedCameraId.value = scanCameras.value[0]?.id ?? ''
    }
  } catch (error) {
    console.error('Failed to enumerate cameras:', error)
    scanError.value = t('scan.cameraError')
  }
}

const startQrScanner = async () => {
  if (!scannerContainer.value) return

  const containerId = 'qr-scanner-' + Date.now()
  scannerContainer.value.id = containerId

  try {
    qrScanner = new Html5Qrcode(containerId)
    await qrScanner.start(
      scanSelectedCameraId.value || { facingMode: 'environment' },
      { fps: 10, qrbox: { width: 250, height: 250 } },
      onQrScanSuccess,
      undefined,
    )
  } catch (error) {
    console.error('Failed to start scanner:', error)
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
}

const refreshScanCameras = async () => {
  await stopQrScanner()
  await loadScanCameras()
  await nextTick()
  startQrScanner()
}

const onQrScanSuccess = async (decodedText: string) => {
  await stopQrScanner()

  try {
    const payload = JSON.parse(decodedText)

    if (!payload.publicKey) {
      scanError.value = t('scan.invalidQr')
      scanStep.value = 'scan'
      await nextTick()
      startQrScanner()
      return
    }

    const existing = await identityStore.getContactByPublicKeyAsync(payload.publicKey)
    scanExistingContact.value = existing ?? null

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

    scanContactNotes.value = ''
    scanStep.value = 'review'
  } catch {
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

    await identityStore.addContactWithClaimsAsync(
      scannedContact.value.label.trim(),
      scannedContact.value.publicKey,
      selectedClaims,
      scanContactNotes.value.trim() || undefined,
    )

    add({ title: t('success.added'), color: 'success' })
    showAddDialog.value = false
  } catch (error) {
    console.error('Failed to save scanned contact:', error)
    add({
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

const onAddContactAsync = async () => {
  if (!addForm.label.trim() || !addForm.publicKey.trim()) return

  isAdding.value = true
  try {
    await identityStore.addContactAsync(
      addForm.label.trim(),
      addForm.publicKey.trim(),
      addForm.notes.trim() || undefined,
    )
    add({ title: t('success.added'), color: 'success' })
    showAddDialog.value = false
    addForm.label = ''
    addForm.publicKey = ''
    addForm.notes = ''
  } catch (error) {
    console.error('Failed to add contact:', error)
    add({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isAdding.value = false
  }
}

const onSelectImportFileAsync = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const { readFile } = await import('@tauri-apps/plugin-fs')

    const filePath = await open({
      title: t('add.title'),
      filters: [{ name: 'JSON', extensions: ['json'] }],
      multiple: false,
    })
    if (!filePath) return

    const data = await readFile(filePath as string)
    importJson.value = new TextDecoder().decode(data)
  } catch (error) {
    console.error('Failed to read file:', error)
    add({
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
    add({ title: t('errors.invalidJson'), color: 'error' })
    return
  }

  if (!parsed.publicKey) {
    add({ title: t('errors.invalidData'), color: 'error' })
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

    const contact = await identityStore.addContactWithClaimsAsync(
      data.label || `Imported ${data.publicKey.slice(0, 16)}...`,
      data.publicKey,
      selectedClaims,
    )
    if (avatar) {
      await identityStore.updateContactAsync(contact.id, { avatar })
    }

    add({ title: t('success.added'), color: 'success' })
    showAddDialog.value = false
    importJson.value = ''
    importParsed.value = null
  } catch (error) {
    console.error('Failed to import contact:', error)
    add({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isAdding.value = false
  }
}

const openEditDialog = (contact: SelectHaexIdentities) => {
  editForm.id = contact.id
  editForm.label = contact.label
  editForm.notes = contact.notes ?? ''
  showEditDialog.value = true
}

const onEditContactAsync = async () => {
  if (!editForm.label.trim()) return

  isEditing.value = true
  try {
    await identityStore.updateContactAsync(editForm.id, {
      label: editForm.label.trim(),
      notes: editForm.notes.trim() || undefined,
    })
    add({ title: t('success.updated'), color: 'success' })
    showEditDialog.value = false
  } catch (error) {
    console.error('Failed to update contact:', error)
    add({
      title: t('errors.updateFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isEditing.value = false
  }
}

const prepareDelete = (contact: SelectHaexIdentities) => {
  deleteTarget.value = contact
  showDeleteConfirm.value = true
}

const onConfirmDeleteAsync = async () => {
  if (!deleteTarget.value) return

  try {
    await identityStore.deleteIdentityAsync(deleteTarget.value.id)
    add({ title: t('success.deleted'), color: 'success' })
    showDeleteConfirm.value = false
    deleteTarget.value = null
  } catch (error) {
    console.error('Failed to delete contact:', error)
    add({
      title: t('errors.deleteFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const copyPublicKey = async (key: string) => {
  try {
    await navigator.clipboard.writeText(key)
    add({ title: t('success.copied'), color: 'success' })
  } catch {
    add({ title: t('errors.copyFailed'), color: 'error' })
  }
}

const copyClaimValue = async (value: string) => {
  try {
    await navigator.clipboard.writeText(value)
    add({ title: t('success.copied'), color: 'success' })
  } catch {
    add({ title: t('errors.copyFailed'), color: 'error' })
  }
}

// Claims management
const expandedContact = ref<string | null>(null)
const contactClaims = ref<
  Record<string, { id: string; type: string; value: string }[]>
>({})
const showClaimDialog = ref(false)
const claimType = ref('email')
const claimCustomType = ref('')
const claimValue = ref('')
const editingClaim = ref<{
  id: string
  contactId: string
  type: string
} | null>(null)
const claimTargetContactId = ref<string | null>(null)

const claimTypeOptions = computed<{ label: string; value: string; disabled?: boolean }[]>(() => [
  { label: 'Email', value: 'email' },
  { label: 'Name', value: 'name' },
  { label: t('claims.custom'), value: 'custom' },
])

const claimValuePlaceholder = computed(() => {
  if (editingClaim.value) return ''
  if (claimType.value === 'email') return 'user@example.com'
  if (claimType.value === 'name') return 'Max Mustermann'
  return ''
})

const canSaveClaim = computed(() => {
  if (!claimValue.value.trim()) return false
  if (
    !editingClaim.value &&
    claimType.value === 'custom' &&
    !claimCustomType.value.trim()
  )
    return false
  return true
})

const onToggleContact = async (contactId: string, open: boolean) => {
  if (!open) {
    expandedContact.value = null
    return
  }
  expandedContact.value = contactId
  await loadClaimsAsync(contactId)
}

const loadClaimsAsync = async (contactId: string) => {
  const claims = await identityStore.getClaimsAsync(contactId)
  contactClaims.value[contactId] = claims.map((c) => ({
    id: c.id,
    type: c.type,
    value: c.value,
  }))
}

const openAddClaim = (contactId: string) => {
  claimTargetContactId.value = contactId
  editingClaim.value = null
  const firstAvailable = claimTypeOptions.value.find((o) => !o.disabled)
  claimType.value = firstAvailable?.value ?? 'custom'
  claimCustomType.value = ''
  claimValue.value = ''
  showClaimDialog.value = true
}

const openEditClaim = (claim: { id: string; type: string; value: string }) => {
  editingClaim.value = {
    id: claim.id,
    contactId: expandedContact.value!,
    type: claim.type,
  }
  claimValue.value = claim.value
  showClaimDialog.value = true
}

const onSaveClaimAsync = async () => {
  if (!canSaveClaim.value) return

  try {
    if (editingClaim.value) {
      await identityStore.updateClaimAsync(
        editingClaim.value.id,
        claimValue.value.trim(),
      )
      await loadClaimsAsync(editingClaim.value.contactId)
      add({ title: t('claims.updated'), color: 'success' })
    } else {
      const type =
        claimType.value === 'custom'
          ? claimCustomType.value.trim()
          : claimType.value
      await identityStore.addClaimAsync(
        claimTargetContactId.value!,
        type,
        claimValue.value.trim(),
      )
      await loadClaimsAsync(claimTargetContactId.value!)
      add({ title: t('claims.added'), color: 'success' })
    }
    showClaimDialog.value = false
  } catch (error) {
    console.error('Failed to save claim:', error)
    add({
      title: t('claims.saveFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const deleteClaimAsync = async (claimId: string, contactId: string) => {
  try {
    await identityStore.deleteClaimAsync(claimId)
    await loadClaimsAsync(contactId)
    add({ title: t('claims.deleted'), color: 'success' })
  } catch (error) {
    console.error('Failed to delete claim:', error)
    add({ title: t('claims.deleteFailed'), color: 'error' })
  }
}

onBeforeUnmount(() => {
  stopQrScanner()
})
</script>

<i18n lang="yaml">
de:
  title: Kontakte
  description: Verwalte deine Kontakte und deren öffentliche Schlüssel
  list:
    title: Deine Kontakte
    description: Kontakte für die Zusammenarbeit in Spaces
    empty: Keine Kontakte vorhanden
    added: Hinzugefügt
  add:
    title: Kontakt hinzufügen
    description: Scanne einen QR-Code, importiere aus einer Datei oder füge manuell hinzu
    tabScan: QR-Code
    tabFile: Aus Datei
    tabManual: Manuell
    selectFile: JSON-Datei auswählen
    orPaste: oder einfügen
    jsonLabel: Kontakt-JSON
    jsonPlaceholder: Exportiertes Identitäts-JSON hier einfügen
    preview: Vorschau
    includeAvatar: Profilbild übernehmen
    selectClaims: Claims zum Importieren auswählen
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
  edit:
    title: Kontakt bearbeiten
  delete:
    title: Kontakt löschen
    description: Möchtest du diesen Kontakt wirklich löschen? Dies kann nicht rückgängig gemacht werden.
  fields:
    label: Name
    publicKey: Public Key
    notes: Notizen
  claims:
    title: Claims
    add: Hinzufügen
    addTitle: Claim hinzufügen
    editTitle: Claim bearbeiten
    type: Typ
    customType: Benutzerdefinierter Typ
    custom: Benutzerdefiniert
    value: Wert
    empty: Keine Claims vorhanden.
    added: Claim hinzugefügt
    updated: Claim aktualisiert
    deleted: Claim gelöscht
    saveFailed: Claim konnte nicht gespeichert werden
    deleteFailed: Claim konnte nicht gelöscht werden
  actions:
    add: Hinzufügen
    share: Teilen
    edit: Bearbeiten
    delete: Löschen
    cancel: Abbrechen
    back: Zurück
    save: Speichern
    copyKey: Public Key kopieren
    toggleClaims: Claims anzeigen/verbergen
  success:
    added: Kontakt hinzugefügt
    updated: Kontakt aktualisiert
    deleted: Kontakt gelöscht
    copied: Kopiert
  errors:
    addFailed: Kontakt konnte nicht hinzugefügt werden
    importFailed: Import fehlgeschlagen
    invalidJson: Ungültiges JSON-Format
    invalidData: Unvollständige Daten (mindestens publicKey erforderlich)
    updateFailed: Aktualisierung fehlgeschlagen
    deleteFailed: Löschen fehlgeschlagen
    copyFailed: Kopieren fehlgeschlagen
en:
  title: Contacts
  description: Manage your contacts and their public keys
  list:
    title: Your Contacts
    description: Contacts for collaboration in Spaces
    empty: No contacts found
    added: Added
  add:
    title: Add Contact
    description: Scan a QR code, import from a file or add manually
    tabScan: QR Code
    tabFile: From file
    tabManual: Manual
    selectFile: Select JSON file
    orPaste: or paste
    jsonLabel: Contact JSON
    jsonPlaceholder: Paste exported identity JSON here
    preview: Preview
    includeAvatar: Include profile picture
    selectClaims: Select claims to import
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
  edit:
    title: Edit Contact
  delete:
    title: Delete Contact
    description: Do you really want to delete this contact? This action cannot be undone.
  fields:
    label: Name
    publicKey: Public Key
    notes: Notes
  claims:
    title: Claims
    add: Add
    addTitle: Add Claim
    editTitle: Edit Claim
    type: Type
    customType: Custom Type
    custom: Custom
    value: Value
    empty: No claims yet.
    added: Claim added
    updated: Claim updated
    deleted: Claim deleted
    saveFailed: Failed to save claim
    deleteFailed: Failed to delete claim
  actions:
    add: Add
    share: Share
    edit: Edit
    delete: Delete
    cancel: Cancel
    back: Back
    save: Save
    copyKey: Copy public key
    toggleClaims: Show/hide claims
  success:
    added: Contact added
    updated: Contact updated
    deleted: Contact deleted
    copied: Copied
  errors:
    addFailed: Failed to add contact
    importFailed: Failed to import file
    invalidJson: Invalid JSON format
    invalidData: Incomplete data (at least publicKey is required)
    updateFailed: Failed to update contact
    deleteFailed: Failed to delete contact
    copyFailed: Failed to copy
</i18n>
