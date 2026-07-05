<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <template #actions>
      <UiButton
        v-if="!showBackendForm"
        icon="i-lucide-plus"
        @click="openAddForm"
      >
        <span class="hidden @sm:inline">
          {{ t('actions.add') }}
        </span>
      </UiButton>
    </template>

    <!-- Add/Edit Backend Form -->
    <UCard v-if="showBackendForm" class="relative">
      <!-- Loading Overlay -->
      <div
        v-if="isLoading"
        class="absolute inset-0 z-10 flex items-center justify-center bg-default/80 backdrop-blur-sm rounded-lg"
      >
        <div class="flex flex-col items-center gap-3">
          <div class="loading loading-spinner loading-lg text-primary" />
          <span class="text-sm text-muted">
            {{ t('form.connecting') }}
          </span>
        </div>
      </div>

      <template #header>
        <div class="flex justify-between px-1">
          <h3 class="text-lg font-semibold">
            {{ isEditMode ? t('editBackend.title') : t('addBackend.title') }}
          </h3>

          <UiButton
            icon="mdi-close"
            variant="ghost"
            color="neutral"
            :disabled="isLoading"
            @click="closeForm"
          />
        </div>
      </template>

      <form class="space-y-4" @submit.prevent="onSubmitFormAsync">
        <UFormField :label="t('form.name.label')" required>
          <UiInput
            v-model="formData.name"
            :placeholder="t('form.name.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.endpoint.label')" :description="t('form.endpoint.description')">
          <UiInput
            v-model="formData.endpoint"
            :placeholder="t('form.endpoint.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.bucket.label')" required>
          <UiInput
            v-model="formData.bucket"
            :placeholder="t('form.bucket.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.region.label')" required>
          <UInputMenu
            v-model="formData.region"
            :items="regionItems"
            :placeholder="t('form.region.placeholder')"
            create-item="always"
            class="w-full"
            @create="onRegionCreate"
          />
        </UFormField>

        <UFormField :label="t('form.accessKeyId.label')" :required="!isEditMode">
          <UiInput
            v-model="formData.accessKeyId"
            :placeholder="isEditMode ? t('form.accessKeyId.keepExisting') : t('form.accessKeyId.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.secretAccessKey.label')" :required="!isEditMode">
          <UiInput
            v-model="formData.secretAccessKey"
            type="password"
            :placeholder="isEditMode ? t('form.secretAccessKey.keepExisting') : t('form.secretAccessKey.placeholder')"
          />
        </UFormField>

        <UFormField>
          <UCheckbox
            v-model="formData.pathStyle"
            :label="t('form.pathStyle.label')"
            :description="t('form.pathStyle.description')"
          />
        </UFormField>
      </form>

      <!--
        K1 — Owner-side "Active Shares" section, only for edits of an
        owned backend. The child component hides itself when there are no
        derived shares, so it stays invisible for freshly-added rows.
      -->
      <StorageActiveSharesSection
        v-if="isEditMode && editingBackendId"
        :parent-backend-id="editingBackendId"
        class="mt-4"
      />

      <template #footer>
        <div class="flex justify-between">
          <UiButton
            color="neutral"
            variant="outline"
            :disabled="isLoading"
            @click="closeForm"
          >
            {{ t('actions.cancel') }}
          </UiButton>

          <UiButton
            :icon="isEditMode ? 'i-lucide-save' : 'mdi-plus'"
            :disabled="isLoading || !isFormValid"
            @click="onSubmitFormAsync"
          >
            <span class="hidden @sm:inline">
              {{ isEditMode ? t('actions.save') : t('actions.add') }}
            </span>
          </UiButton>
        </div>
      </template>
    </UCard>

    <!-- Storage Backends List -->
    <div
      v-if="!showBackendForm || storageBackends.length"
    >

      <UiListContainer v-if="storageBackends.length">
        <UiListItem
          v-for="backend in storageBackends"
          :key="backend.id"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 flex-wrap">
              <h4 class="font-medium">{{ backend.name }}</h4>
              <UBadge
                :color="backend.enabled ? 'success' : 'neutral'"
                variant="subtle"
              >
                {{ backend.enabled ? t('backends.enabled') : t('backends.disabled') }}
              </UBadge>
              <UBadge
                v-if="isSharedBackend(backend)"
                color="info"
                variant="subtle"
                icon="i-lucide-share-2"
              >
                {{ t('backends.sharedFrom', { space: backend.spaceName ?? t('backends.unknownSpace') }) }}
              </UBadge>
              <UBadge
                v-if="isSharedBackend(backend)"
                :color="accessLevelBadgeColor(backend.shareAccessFlags)"
                variant="subtle"
              >
                {{ accessLevelBadgeLabel(backend.shareAccessFlags) }}
              </UBadge>
            </div>
            <div class="text-sm text-muted mt-1 space-y-0.5">
              <p v-if="backend.config?.endpoint">
                <span class="font-medium">{{ t('form.endpoint.label') }}:</span>
                {{ backend.config.endpoint }}
              </p>
              <p>
                <span class="font-medium">{{ t('form.bucket.label') }}:</span>
                {{ backend.config?.bucket }}
              </p>
              <p>
                <span class="font-medium">{{ t('form.region.label') }}:</span>
                {{ backend.config?.region }}
              </p>
            </div>
          </div>

          <template #actions>
            <UiButton
              color="neutral"
              variant="outline"
              :loading="testingBackendId === backend.id"
              :disabled="testingBackendId !== null"
              @click="onTestBackendAsync(backend.id)"
            >
              {{ t('actions.test') }}
            </UiButton>
            <!--
              Edit + delete are hidden on shared entries — the credentials
              belong to the owner's device, and a member "removing" the row
              would just resurface on the next sync. Design doc §7.3.
            -->
            <UiButton
              v-if="!isSharedBackend(backend)"
              color="neutral"
              variant="ghost"
              icon="i-lucide-pencil"
              @click="openEditForm(backend)"
            />
            <UiButton
              v-if="!isSharedBackend(backend)"
              color="error"
              variant="ghost"
              icon="i-lucide-trash-2"
              @click="prepareDeleteBackend(backend)"
            />
          </template>
        </UiListItem>
      </UiListContainer>

      <HaexSystemSettingsLayoutEmpty
        v-else
        :message="t('backends.noBackends')"
        icon="i-heroicons-cloud"
      />
    </div>

    <!-- Delete Confirmation Dialog (no active shares) -->
    <UiDialogConfirm
      v-model:open="showDeleteDialog"
      :title="t('deleteBackend.title')"
      :description="t('deleteBackend.description', { name: backendToDelete?.name })"
      :confirm-label="t('actions.delete')"
      @confirm="onConfirmDeleteAsync"
    />

    <!--
      L0 — Cascade-delete confirmation. Shown when the owner is about to
      delete an owned backend that still has active share rows. All accesses
      are revoked one-by-one before the parent row is removed. Body slot
      lists the affected spaces + shows live progress; a revoke failure
      aborts before the parent delete so partial state stays retry-safe.
    -->
    <UiDialogConfirm
      v-model:open="showCascadeDeleteDialog"
      :title="t('cascadeDeleteBackend.title')"
      :confirm-label="t('actions.delete')"
      :confirm-disabled="isCascadeDeleting"
      @confirm="onConfirmCascadeDeleteAsync"
    >
      <template #body>
        <div class="space-y-3">
          <p class="text-sm">
            {{ t('cascadeDeleteBackend.body', {
              name: backendToDelete?.name,
              count: activeShares.length,
            }) }}
          </p>
          <div>
            <p class="text-xs font-semibold text-muted uppercase tracking-wide mb-1">
              {{ t('cascadeDeleteBackend.affectedSpaces') }}
            </p>
            <ul class="text-sm space-y-1">
              <li
                v-for="share in activeShares"
                :key="share.id"
                class="flex items-center gap-2"
              >
                <UIcon
                  name="i-lucide-users"
                  class="w-3.5 h-3.5 shrink-0 text-primary"
                />
                <span class="truncate">{{ share.spaceName }}</span>
              </li>
            </ul>
          </div>
          <p class="text-xs text-muted">
            {{ t('cascadeDeleteBackend.irreversible') }}
          </p>
          <div
            v-if="isCascadeDeleting"
            class="flex items-center gap-2 rounded-md bg-primary/10 px-3 py-2"
          >
            <UIcon
              name="i-lucide-loader-2"
              class="w-4 h-4 animate-spin text-primary"
            />
            <span class="text-xs">
              {{ cascadeProgressLabel }}
            </span>
          </div>
        </div>
      </template>
    </UiDialogConfirm>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { and, eq } from 'drizzle-orm'
import type { StorageBackendInfo } from '~/../src-tauri/bindings/StorageBackendInfo'
import type { AddStorageBackendRequest } from '~/../src-tauri/bindings/AddStorageBackendRequest'
import type { UpdateStorageBackendRequest } from '~/../src-tauri/bindings/UpdateStorageBackendRequest'
import {
  accessLevelBadgeColor,
  accessLevelKind,
} from '~/lib/storage/shareAccessFlags'
import {
  haexS3Backends,
  haexSharedSpaceSync,
  haexSpaces,
} from '~/database/schemas'
import StorageActiveSharesSection from './storage/StorageActiveSharesSection.vue'

const { t } = useI18n()
const { add } = useToast()
const { getDb } = useVaultDb()
const { revokeBackend } = useStorageSharing()
const fileSyncStore = useFileSyncStore()

const HAEX_S3_BACKENDS_TABLE = 'haex_s3_backends'

const DEFAULT_S3_REGION = 'auto'

const KNOWN_S3_REGIONS = [
  'auto',
  'us-east-1',
  'us-east-2',
  'us-west-1',
  'us-west-2',
  'af-south-1',
  'ap-east-1',
  'ap-south-1',
  'ap-south-2',
  'ap-southeast-1',
  'ap-southeast-2',
  'ap-southeast-3',
  'ap-southeast-4',
  'ap-northeast-1',
  'ap-northeast-2',
  'ap-northeast-3',
  'ca-central-1',
  'eu-central-1',
  'eu-central-2',
  'eu-west-1',
  'eu-west-2',
  'eu-west-3',
  'eu-south-1',
  'eu-south-2',
  'eu-north-1',
  'me-south-1',
  'me-central-1',
  'sa-east-1',
] as const

const regionItems = ref<string[]>([...KNOWN_S3_REGIONS])

// State
const storageBackends = ref<StorageBackendInfo[]>([])
const showBackendForm = ref(false)
const isEditMode = ref(false)
const editingBackendId = ref<string | null>(null)
const isLoading = ref(false)
const testingBackendId = ref<string | null>(null)
const showDeleteDialog = ref(false)
const showCascadeDeleteDialog = ref(false)
const backendToDelete = ref<StorageBackendInfo | null>(null)

interface ActiveShareEntry {
  id: string
  spaceId: string
  spaceName: string
}
const activeShares = ref<ActiveShareEntry[]>([])
const isCascadeDeleting = ref(false)
const cascadeProgress = ref<{ current: number; total: number; spaceName: string } | null>(null)

const cascadeProgressLabel = computed(() => {
  const p = cascadeProgress.value
  if (!p) return t('cascadeDeleteBackend.progress.starting')
  return t('cascadeDeleteBackend.progress.revoking', {
    current: p.current,
    total: p.total,
    name: p.spaceName,
  })
})

const formData = reactive({
  name: '',
  endpoint: '',
  bucket: '',
  region: DEFAULT_S3_REGION,
  accessKeyId: '',
  secretAccessKey: '',
  pathStyle: false,
})

/**
 * True when a backend row was replicated to us as a space member (owner's
 * device wrote it, we received it via the CRDT sync of
 * `haex_shared_space_sync`). Rows written before the origin_type migration
 * report `null` — treat those as `owned` to match the pre-existing UX.
 */
const isSharedBackend = (backend: StorageBackendInfo): boolean =>
  backend.originType === 'shared_from_space'

const accessLevelBadgeLabel = (flags: number | null | undefined): string =>
  t(`backends.accessLevel.${accessLevelKind(flags)}`)

const onRegionCreate = (value: string) => {
  const trimmed = value.trim()
  if (!trimmed) return
  if (!regionItems.value.includes(trimmed)) {
    regionItems.value.push(trimmed)
  }
  formData.region = trimmed
}

const isFormValid = computed(() => {
  const baseValid =
    formData.name.trim() !== '' &&
    formData.bucket.trim() !== '' &&
    formData.region.trim() !== ''

  // In edit mode, credentials are optional (keep existing)
  if (isEditMode.value) {
    return baseValid
  }

  // In add mode, credentials are required
  return (
    baseValid &&
    formData.accessKeyId.trim() !== '' &&
    formData.secretAccessKey.trim() !== ''
  )
})

const getErrorMessage = (error: unknown): string => {
  const raw = (() => {
    if (error instanceof Error) return error.message
    if (typeof error === 'string') return error
    if (error && typeof error === 'object') {
      const details = (error as { details?: unknown }).details
      if (details && typeof details === 'object') {
        const reason = (details as { reason?: unknown }).reason
        if (typeof reason === 'string') return reason
      }
      try {
        return JSON.stringify(error)
      } catch {
        return String(error)
      }
    }
    return String(error)
  })()

  const s3Message = raw.match(/<Message>([^<]+)<\/Message>/)?.[1]
  if (s3Message) return s3Message

  return raw.length > 240 ? `${raw.slice(0, 237)}...` : raw
}

// Load backends on mount
onMounted(async () => {
  await loadBackendsAsync()
})

const loadBackendsAsync = async () => {
  try {
    storageBackends.value = await invoke<StorageBackendInfo[]>('remote_storage_list_backends')
  } catch (error) {
    console.error('Failed to load storage backends:', error)
    add({
      title: t('errors.loadFailed'),
      description: getErrorMessage(error),
      color: 'error',
    })
  }
}

const resetForm = () => {
  formData.name = ''
  formData.endpoint = ''
  formData.bucket = ''
  formData.region = DEFAULT_S3_REGION
  formData.accessKeyId = ''
  formData.secretAccessKey = ''
  formData.pathStyle = false
  isEditMode.value = false
  editingBackendId.value = null
}

const openAddForm = () => {
  resetForm()
  showBackendForm.value = true
}

const openEditForm = (backend: StorageBackendInfo) => {
  resetForm()
  isEditMode.value = true
  editingBackendId.value = backend.id
  formData.name = backend.name
  formData.endpoint = backend.config?.endpoint || ''
  formData.bucket = backend.config?.bucket || ''
  formData.region = backend.config?.region || DEFAULT_S3_REGION
  if (formData.region && !regionItems.value.includes(formData.region)) {
    regionItems.value.push(formData.region)
  }
  // Credentials are not returned from the backend for security
  formData.accessKeyId = ''
  formData.secretAccessKey = ''
  formData.pathStyle = backend.config?.pathStyle || false
  showBackendForm.value = true
}

const closeForm = () => {
  showBackendForm.value = false
  resetForm()
}

const onSubmitFormAsync = async () => {
  if (!isFormValid.value) return

  isLoading.value = true

  try {
    if (isEditMode.value && editingBackendId.value) {
      await onUpdateBackendAsync()
    } else {
      await onAddBackendAsync()
    }
  } finally {
    isLoading.value = false
  }
}

const onAddBackendAsync = async () => {
  try {
    const config: Record<string, unknown> = {
      bucket: formData.bucket.trim(),
      region: formData.region.trim(),
      accessKeyId: formData.accessKeyId.trim(),
      secretAccessKey: formData.secretAccessKey.trim(),
    }

    if (formData.endpoint.trim()) {
      config.endpoint = formData.endpoint.trim()
    }

    if (formData.pathStyle) {
      config.pathStyle = true
    }

    const request: AddStorageBackendRequest = {
      name: formData.name.trim(),
      type: 's3',
      config,
    }

    await invoke('remote_storage_add_backend', { request })

    add({
      title: t('success.backendAdded'),
      color: 'success',
    })

    await loadBackendsAsync()
    closeForm()
  } catch (error) {
    console.error('Failed to add storage backend:', error)
    add({
      title: t('errors.addFailed'),
      description: getErrorMessage(error),
      color: 'error',
    })
  }
}

const onUpdateBackendAsync = async () => {
  if (!editingBackendId.value) return

  try {
    const config: Record<string, unknown> = {
      bucket: formData.bucket.trim(),
      region: formData.region.trim(),
    }

    if (formData.endpoint.trim()) {
      config.endpoint = formData.endpoint.trim()
    }

    // Always send pathStyle (both true and false)
    config.pathStyle = formData.pathStyle

    // Only include credentials if provided (otherwise keep existing)
    if (formData.accessKeyId.trim()) {
      config.accessKeyId = formData.accessKeyId.trim()
    }
    if (formData.secretAccessKey.trim()) {
      config.secretAccessKey = formData.secretAccessKey.trim()
    }

    const request: UpdateStorageBackendRequest = {
      backendId: editingBackendId.value,
      name: formData.name.trim(),
      config,
    }

    await invoke('remote_storage_update_backend', { request })

    add({
      title: t('success.backendUpdated'),
      color: 'success',
    })

    // Running sync loops hold a constructed S3 client built from the old
    // config — restart any rule that references this backend so it picks
    // up the new region/endpoint/credentials.
    try {
      const restarted = await fileSyncStore.restartRulesUsingBackendAsync(editingBackendId.value!)
      if (restarted > 0) {
        add({
          title: t('success.rulesRestarted', { count: restarted }),
          color: 'success',
        })
      }
    } catch (error) {
      console.error('Failed to restart sync rules after backend update:', error)
    }

    await loadBackendsAsync()
    closeForm()
  } catch (error) {
    console.error('Failed to update storage backend:', error)
    add({
      title: t('errors.updateFailed'),
      description: getErrorMessage(error),
      color: 'error',
    })
  }
}

const onTestBackendAsync = async (backendId: string) => {
  testingBackendId.value = backendId

  try {
    await invoke('remote_storage_test_backend', { backendId })
    add({
      title: t('success.connectionOk'),
      color: 'success',
    })
  } catch (error) {
    console.error('Connection test failed:', error)
    add({
      title: t('errors.testFailed'),
      description: getErrorMessage(error),
      color: 'error',
    })
  } finally {
    testingBackendId.value = null
  }
}

/**
 * L0 — Look up owner-side share derivations for a parent backend. Mirrors
 * K1's query pattern: rows in `haex_s3_backends` with
 * `origin_type='shared_from_space'` and `parent_backend_id=parentId`, joined
 * to the space they landed in via `haex_shared_space_sync` (rowPks[0] holds
 * the shared row's id).
 */
const loadActiveSharesAsync = async (
  parentId: string,
): Promise<ActiveShareEntry[]> => {
  const db = getDb()
  if (!db) return []

  const rows = await db
    .select()
    .from(haexS3Backends)
    .where(
      and(
        eq(haexS3Backends.parentBackendId, parentId),
        eq(haexS3Backends.originType, 'shared_from_space'),
      ),
    )

  if (rows.length === 0) return []

  const assignments = await db
    .select()
    .from(haexSharedSpaceSync)
    .where(eq(haexSharedSpaceSync.tableName, HAEX_S3_BACKENDS_TABLE))

  const spaceByBackend = new Map<string, string>()
  for (const a of assignments) {
    const pks = Array.isArray(a.rowPks) ? a.rowPks : []
    const first = pks[0]
    if (typeof first === 'string') spaceByBackend.set(first, a.spaceId)
  }

  const spaceIds = new Set(spaceByBackend.values())
  const spaceNameById = new Map<string, string>()
  if (spaceIds.size > 0) {
    const spaces = await db.select().from(haexSpaces)
    for (const s of spaces) {
      if (spaceIds.has(s.id)) spaceNameById.set(s.id, s.name)
    }
  }

  return rows.flatMap((r): ActiveShareEntry[] => {
    const spaceId = spaceByBackend.get(r.id)
    if (!spaceId) return []
    return [{
      id: r.id,
      spaceId,
      spaceName: spaceNameById.get(spaceId) ?? t('cascadeDeleteBackend.unknownSpace'),
    }]
  })
}

const prepareDeleteBackend = async (backend: StorageBackendInfo) => {
  backendToDelete.value = backend

  // Look up active shares first. Any non-empty result routes through the
  // cascade-warning dialog; empty routes through the existing simple confirm.
  try {
    activeShares.value = await loadActiveSharesAsync(backend.id)
  } catch (error) {
    // Abort instead of falling back to the simple confirm: deleting the
    // parent while shares exist cascades the DB rows away but leaves the
    // scoped IAM users live at the provider with no record to revoke them.
    console.error('Failed to look up active shares:', error)
    add({
      title: t('errors.shareLookupFailed'),
      description: getErrorMessage(error),
      color: 'error',
    })
    backendToDelete.value = null
    activeShares.value = []
    return
  }

  if (activeShares.value.length > 0) {
    showCascadeDeleteDialog.value = true
  } else {
    showDeleteDialog.value = true
  }
}

const deleteParentBackendAsync = async (backendId: string): Promise<void> => {
  await invoke('remote_storage_remove_backend', { backendId })
}

const onConfirmDeleteAsync = async () => {
  if (!backendToDelete.value) return

  try {
    await deleteParentBackendAsync(backendToDelete.value.id)

    add({
      title: t('success.backendDeleted'),
      color: 'success',
    })

    await loadBackendsAsync()
  } catch (error) {
    console.error('Failed to delete storage backend:', error)
    add({
      title: t('errors.deleteFailed'),
      description: getErrorMessage(error),
      color: 'error',
    })
  } finally {
    showDeleteDialog.value = false
    backendToDelete.value = null
  }
}

/**
 * L0 — Cascade path. Revoke every share sequentially, then delete the
 * parent. A revoke failure aborts BEFORE the parent delete so the row +
 * remaining shares stay retry-safe (already-revoked shares are permanently
 * gone, which matches revoke_storage_share's own idempotency contract).
 */
const onConfirmCascadeDeleteAsync = async () => {
  if (!backendToDelete.value) return
  if (isCascadeDeleting.value) return

  const parentId = backendToDelete.value.id
  const shares = [...activeShares.value]

  isCascadeDeleting.value = true
  cascadeProgress.value = null

  try {
    for (let i = 0; i < shares.length; i++) {
      const share = shares[i]!
      cascadeProgress.value = {
        current: i + 1,
        total: shares.length,
        spaceName: share.spaceName,
      }
      try {
        await revokeBackend(share.id)
      } catch (error) {
        console.error('Failed to revoke share during cascade delete:', error)
        add({
          title: t('errors.cascadeRevokeFailed', { name: share.spaceName }),
          description: getErrorMessage(error),
          color: 'error',
        })
        // Abort before parent delete — leaves the (now shorter) share set
        // + parent intact so the user can retry.
        await loadBackendsAsync()
        return
      }
    }

    // All shares revoked — now delete the parent row.
    try {
      await deleteParentBackendAsync(parentId)
    } catch (error) {
      console.error('Failed to delete storage backend after cascade revoke:', error)
      add({
        title: t('errors.cascadeDeleteAfterRevokeFailed'),
        description: getErrorMessage(error),
        color: 'error',
      })
      await loadBackendsAsync()
      return
    }

    add({
      title: t('success.cascadeDeleted', { count: shares.length }),
      color: 'success',
    })
    await loadBackendsAsync()
  } finally {
    isCascadeDeleting.value = false
    cascadeProgress.value = null
    showCascadeDeleteDialog.value = false
    backendToDelete.value = null
    activeShares.value = []
  }
}
</script>

<i18n lang="yaml">
de:
  title: Cloud Storage
  description: Verwalte S3-kompatible Storage Backends für Erweiterungen
  addBackend:
    title: Cloud Storage Backend hinzufügen
  editBackend:
    title: Cloud Storage Backend bearbeiten
  backends:
    title: Cloud Storage Backends
    description: S3-kompatible Speicherdienste für Datei-Uploads
    noBackends: Keine Storage Backends konfiguriert
    noBackendsHint: Füge ein S3-kompatibles Backend hinzu, um Dateien zu speichern
    enabled: Aktiviert
    disabled: Deaktiviert
    sharedFrom: "aus {space}"
    unknownSpace: Space
    accessLevel:
      readOnly: nur lesen
      readWrite: lesen + schreiben
      custom: benutzerdefiniert
  form:
    connecting: Verbindung wird getestet...
    name:
      label: Name
      placeholder: Mein S3 Speicher
    endpoint:
      label: Endpoint
      description: Nur für S3-kompatible Dienste wie MinIO, Cloudflare R2, etc.
      placeholder: https://s3.example.com
    bucket:
      label: Bucket
      placeholder: my-bucket
    region:
      label: Region
      placeholder: auto
    accessKeyId:
      label: Access Key ID
      placeholder: AKIAIOSFODNN7EXAMPLE
      keepExisting: Leer lassen um bestehenden Key zu behalten
    secretAccessKey:
      label: Secret Access Key
      placeholder: "********"
      keepExisting: Leer lassen um bestehendes Secret zu behalten
    pathStyle:
      label: Path-Style URLs verwenden
      description: Aktivieren für MinIO und andere S3-kompatible Dienste
  actions:
    add: Hinzufügen
    save: Speichern
    cancel: Abbrechen
    test: Testen
    delete: Löschen
  deleteBackend:
    title: Cloud Storage Backend löschen
    description: Möchtest du das Backend "{name}" wirklich löschen? Erweiterungen können dann nicht mehr auf dieses Backend zugreifen.
  cascadeDeleteBackend:
    title: Cloud-Speicher löschen?
    body: "Der Bucket \"{name}\" wird aktuell in {count} Space(s) geteilt. Beim Löschen werden alle Zugänge widerrufen und die Freigaben aus den Spaces entfernt."
    affectedSpaces: "Betroffene Spaces:"
    irreversible: Diese Aktion kann nicht rückgängig gemacht werden.
    unknownSpace: Unbekannter Space
    progress:
      starting: Freigaben werden vorbereitet …
      revoking: "Widerrufe Zugriff für Space {name} ({current}/{total}) …"
  success:
    backendAdded: Storage Backend hinzugefügt
    backendUpdated: Storage Backend aktualisiert
    backendDeleted: Storage Backend gelöscht
    cascadeDeleted: "Backend gelöscht — {count} Freigabe(n) widerrufen"
    connectionOk: Verbindung erfolgreich
    rulesRestarted: "{count} Sync-Regel(n) mit neuer Konfiguration neu gestartet"
  errors:
    loadFailed: Backends konnten nicht geladen werden
    addFailed: Backend konnte nicht hinzugefügt werden
    updateFailed: Backend konnte nicht aktualisiert werden
    deleteFailed: Backend konnte nicht gelöscht werden
    testFailed: Verbindungstest fehlgeschlagen
    cascadeRevokeFailed: "Zugriff für Space \"{name}\" konnte nicht widerrufen werden — Löschen abgebrochen"
    cascadeDeleteAfterRevokeFailed: "Freigaben wurden widerrufen, aber das Backend konnte nicht gelöscht werden — bitte erneut versuchen"
    shareLookupFailed: "Aktive Freigaben konnten nicht ermittelt werden — Löschen abgebrochen"
en:
  title: Cloud Storage
  description: Manage S3-compatible storage backends for extensions
  addBackend:
    title: Add Storage Backend
  editBackend:
    title: Edit Storage Backend
  backends:
    title: Cloud Storage Backends
    description: S3-compatible storage services for file uploads
    noBackends: No storage backends configured
    noBackendsHint: Add an S3-compatible backend to store files
    enabled: Enabled
    disabled: Disabled
    sharedFrom: "shared from {space}"
    unknownSpace: Space
    accessLevel:
      readOnly: read-only
      readWrite: read + write
      custom: custom
  form:
    connecting: Testing connection...
    name:
      label: Name
      placeholder: My S3 Storage
    endpoint:
      label: Endpoint
      description: Only for S3-compatible services like MinIO, Cloudflare R2, etc.
      placeholder: https://s3.example.com
    bucket:
      label: Bucket
      placeholder: my-bucket
    region:
      label: Region
      placeholder: auto
    accessKeyId:
      label: Access Key ID
      placeholder: AKIAIOSFODNN7EXAMPLE
      keepExisting: Leave empty to keep existing key
    secretAccessKey:
      label: Secret Access Key
      placeholder: "********"
      keepExisting: Leave empty to keep existing secret
    pathStyle:
      label: Use path-style URLs
      description: Enable for MinIO and other S3-compatible services
  actions:
    add: Add
    save: Save
    cancel: Cancel
    test: Test
    delete: Delete
  deleteBackend:
    title: Delete Storage Backend
    description: Do you really want to delete the backend "{name}"? Extensions will no longer be able to access this backend.
  cascadeDeleteBackend:
    title: Delete cloud storage?
    body: "The bucket \"{name}\" is currently shared in {count} space(s). Deleting it will revoke all accesses and remove the shares from those spaces."
    affectedSpaces: "Affected spaces:"
    irreversible: This action cannot be undone.
    unknownSpace: Unknown space
    progress:
      starting: Preparing shares …
      revoking: "Revoking access for space {name} ({current}/{total}) …"
  success:
    backendAdded: Storage backend added
    backendUpdated: Storage backend updated
    backendDeleted: Storage backend deleted
    cascadeDeleted: "Backend deleted — revoked {count} share(s)"
    connectionOk: Connection successful
    rulesRestarted: "Restarted {count} sync rule(s) with new configuration"
  errors:
    loadFailed: Failed to load backends
    addFailed: Failed to add backend
    updateFailed: Failed to update backend
    deleteFailed: Failed to delete backend
    testFailed: Connection test failed
    cascadeRevokeFailed: "Failed to revoke access for space \"{name}\" — delete aborted"
    cascadeDeleteAfterRevokeFailed: "Shares were revoked but the backend could not be deleted — please try again"
    shareLookupFailed: "Could not determine active shares — delete aborted"
</i18n>
