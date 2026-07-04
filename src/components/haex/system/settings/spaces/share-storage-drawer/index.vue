<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
  >
    <template #body>
      <div class="space-y-4">
        <UTabs
          v-model="activeTab"
          :items="tabItems"
          class="w-full"
        />

        <TabScope
          v-show="activeTab === 'scope'"
          v-model:selected-backend-id="selectedBackendId"
          v-model:selected-prefix="selectedPrefix"
          :backends="backends"
          :loading="isLoadingBackends"
        />

        <TabPermissions
          v-show="activeTab === 'permissions'"
          v-model:access-flags="accessFlags"
          :space-name="spaceName"
        />
      </div>
    </template>

    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          :disabled="isSubmitting"
          @click="onCancel"
        >
          {{ t('cancel') }}
        </UButton>
        <UiButton
          icon="i-lucide-share-2"
          :loading="isSubmitting"
          :disabled="!canConfirm"
          @click="onConfirmAsync"
        >
          {{ t('confirm') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>

  <!-- IAM admin credential modal — surfaced when the initial shareBackend()
       returns IamAdminCredMissing. On submit we retry the share with the
       hint populated so the vault can store the credential and proceed. -->
  <UModal v-model:open="showIamCredModal">
    <template #content>
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-base font-semibold">{{ t('iam.title') }}</h3>
            <UiButton
              color="neutral"
              variant="ghost"
              icon="i-lucide-x"
              :disabled="isSubmitting"
              @click="showIamCredModal = false"
            />
          </div>
        </template>

        <div class="space-y-3">
          <p class="text-sm text-muted">{{ t('iam.description') }}</p>

          <UiInput
            v-model="iamForm.accessKeyId"
            :label="t('iam.accessKeyId')"
            autocomplete="off"
          />
          <UiInput
            v-model="iamForm.secretAccessKey"
            :label="t('iam.secretAccessKey')"
            type="password"
            autocomplete="off"
          />

          <div class="space-y-1">
            <label class="text-sm font-medium">{{ t('iam.provider') }}</label>
            <USelectMenu
              v-model="iamProviderOption"
              :items="providerOptions"
              :placeholder="t('iam.providerPlaceholder')"
              by="value"
              class="w-full"
            />
          </div>
        </div>

        <template #footer>
          <div class="flex justify-between gap-4">
            <UButton
              color="neutral"
              variant="outline"
              :disabled="isSubmitting"
              @click="showIamCredModal = false"
            >
              {{ t('cancel') }}
            </UButton>
            <UiButton
              icon="i-lucide-check"
              :loading="isSubmitting"
              :disabled="!canSubmitIamForm"
              @click="onIamSubmitAsync"
            >
              {{ t('iam.submit') }}
            </UiButton>
          </div>
        </template>
      </UCard>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import TabScope from './tab-scope.vue'
import TabPermissions from './tab-permissions.vue'
import {
  useStorageSharing,
  type StorageError,
  type StorageProviderKind,
  type IamAdminCredHint,
} from '@/composables/useStorageSharing'
import { SHARE_ACCESS_READ_ONLY } from '~/lib/storage/shareAccessFlags'
import { haexS3Backends } from '~/database/schemas'
import type { SelectHaexS3Backends } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

const props = defineProps<{
  spaceId: string
  spaceName: string
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { add } = useToast()
const { shareBackend } = useStorageSharing()

const activeTab = ref<'scope' | 'permissions'>('scope')

const tabItems = computed(() => [
  { label: t('tabs.scope'), value: 'scope' as const },
  { label: t('tabs.permissions'), value: 'permissions' as const },
])

// --- Scope state ------------------------------------------------------------
const backends = ref<SelectHaexS3Backends[]>([])
const isLoadingBackends = ref(false)
const selectedBackendId = ref<string | null>(null)
// undefined = whole bucket (Rust `Option<String>::None`).
// non-undefined string (may be empty until the user picks a folder) = prefix
// scope; the share button is disabled while it's still empty.
const selectedPrefix = ref<string | undefined>(undefined)

const loadBackendsAsync = async () => {
  isLoadingBackends.value = true
  try {
    const db = requireDb()
    backends.value = await db
      .select()
      .from(haexS3Backends)
      .where(eq(haexS3Backends.originType, 'owned'))
  } catch (error) {
    console.error('Failed to load owned S3 backends:', error)
    backends.value = []
  } finally {
    isLoadingBackends.value = false
  }
}

// --- Permissions state ------------------------------------------------------
const accessFlags = ref<number>(SHARE_ACCESS_READ_ONLY)

// --- IAM cred modal state --------------------------------------------------
const showIamCredModal = ref(false)
const iamForm = reactive({
  accessKeyId: '',
  secretAccessKey: '',
})
type ProviderOption = {
  label: string
  value: StorageProviderKind
  disabled?: boolean
}
const providerOptions = computed<ProviderOption[]>(() => [
  { label: t('iam.providerAws'), value: 'aws' },
  { label: t('iam.providerWasabi'), value: 'wasabi' },
  {
    label: t('iam.providerMinioDisabled'),
    value: 'minio',
    disabled: true,
  },
])
const iamProviderOption = ref<ProviderOption | undefined>()

const canSubmitIamForm = computed(
  () =>
    iamForm.accessKeyId.trim().length > 0
    && iamForm.secretAccessKey.length > 0
    && iamProviderOption.value !== undefined
    && !iamProviderOption.value.disabled,
)

// --- Submission -------------------------------------------------------------
const isSubmitting = ref(false)

const canConfirm = computed(
  () =>
    selectedBackendId.value !== null
    && accessFlags.value !== 0
    // In prefix mode the user must actually pick a folder before we let
    // them submit — an empty prefix would either widen the share to the
    // whole bucket accidentally or fail server-side.
    && selectedPrefix.value !== ''
    && !isSubmitting.value,
)

const resetState = () => {
  activeTab.value = 'scope'
  selectedBackendId.value = null
  selectedPrefix.value = undefined
  accessFlags.value = SHARE_ACCESS_READ_ONLY
  showIamCredModal.value = false
  iamForm.accessKeyId = ''
  iamForm.secretAccessKey = ''
  iamProviderOption.value = undefined
}

const onCancel = () => {
  open.value = false
}

const isStorageError = (e: unknown): e is StorageError =>
  typeof e === 'object'
  && e !== null
  && 'type' in e
  && typeof (e as { type: unknown }).type === 'string'

const runShareAsync = async (hint?: IamAdminCredHint) => {
  if (!selectedBackendId.value) return
  isSubmitting.value = true
  try {
    await shareBackend({
      storageId: selectedBackendId.value,
      spaceId: props.spaceId,
      // Only forward a prefix if the user actually picked a folder. Empty
      // string means "user opened the folder mode but hasn't chosen yet" —
      // canConfirm blocks the button in that state, but we still guard here.
      prefix: selectedPrefix.value ? selectedPrefix.value : undefined,
      accessFlags: accessFlags.value,
      iamAdminCredHint: hint,
    })
    add({ title: t('success'), color: 'success' })
    open.value = false
  } catch (error) {
    if (isStorageError(error) && error.type === 'IamAdminCredMissing') {
      showIamCredModal.value = true
    } else {
      console.error('Failed to share storage backend:', error)
      const message = isStorageError(error)
        ? t(`errors.${error.type}`, t('errors.generic'))
        : t('errors.generic')
      add({ title: message, color: 'error' })
    }
  } finally {
    isSubmitting.value = false
  }
}

const onConfirmAsync = () => runShareAsync()

const onIamSubmitAsync = () => {
  if (!canSubmitIamForm.value || !iamProviderOption.value) return
  const hint: IamAdminCredHint = {
    accessKeyId: iamForm.accessKeyId.trim(),
    secretAccessKey: iamForm.secretAccessKey,
    providerType: iamProviderOption.value.value,
  }
  showIamCredModal.value = false
  return runShareAsync(hint)
}

// Load backends whenever the drawer opens; reset state whenever it closes.
watch(open, (isOpen, wasOpen) => {
  if (isOpen && !wasOpen) {
    resetState()
    void loadBackendsAsync()
  }
})
</script>

<i18n lang="yaml">
de:
  title: S3-Bucket teilen
  tabs:
    scope: Bereich
    permissions: Berechtigungen
  cancel: Abbrechen
  confirm: Freigeben
  success: Bucket freigegeben
  errors:
    generic: Freigabe fehlgeschlagen
    BackendNotFound: Backend nicht gefunden
    StorageNotFound: Storage nicht gefunden
    InvalidArgs: Ungültige Argumente
    InvalidConfig: Ungültige Konfiguration
    ConnectionFailed: Verbindung zum Anbieter fehlgeschlagen
    IamAdminInsufficient: Die Admin-Zugangsdaten haben nicht ausreichende Rechte
    UnsupportedProvider: Anbieter wird nicht unterstützt
    IamOperationFailed: IAM-Operation fehlgeschlagen
    ObjectScopeNotYetSupported: Einzelne Objekte können noch nicht freigegeben werden
    NotAShareRow: Zeile ist keine Freigabe
    ParentBackendMissing: Übergeordnetes Backend fehlt
    DatabaseError: Datenbankfehler
    Internal: Interner Fehler
  iam:
    title: Admin-Zugangsdaten benötigt
    description: Für die einmalige Provisionierung eines eingeschränkten Nutzers werden Admin-Zugangsdaten des Cloud-Providers benötigt. Sie werden im Vault verschlüsselt gespeichert.
    accessKeyId: Access Key ID
    secretAccessKey: Secret Access Key
    provider: Anbieter
    providerPlaceholder: Anbieter wählen
    providerAws: AWS
    providerWasabi: Wasabi
    providerMinioDisabled: MinIO (noch nicht unterstützt)
    submit: Übernehmen und freigeben
en:
  title: Share S3 bucket
  tabs:
    scope: Scope
    permissions: Permissions
  cancel: Cancel
  confirm: Share
  success: Bucket shared
  errors:
    generic: Sharing failed
    BackendNotFound: Backend not found
    StorageNotFound: Storage not found
    InvalidArgs: Invalid arguments
    InvalidConfig: Invalid configuration
    ConnectionFailed: Failed to connect to provider
    IamAdminInsufficient: The admin credentials have insufficient permissions
    UnsupportedProvider: Provider not supported
    IamOperationFailed: IAM operation failed
    ObjectScopeNotYetSupported: Single-object scope is not yet supported
    NotAShareRow: Row is not a share
    ParentBackendMissing: Parent backend missing
    DatabaseError: Database error
    Internal: Internal error
  iam:
    title: Admin credentials required
    description: Provisioning a scoped user needs one-time admin credentials for the cloud provider. They will be stored encrypted inside the vault.
    accessKeyId: Access Key ID
    secretAccessKey: Secret Access Key
    provider: Provider
    providerPlaceholder: Select provider
    providerAws: AWS
    providerWasabi: Wasabi
    providerMinioDisabled: MinIO (not yet supported)
    submit: Save and share
</i18n>
