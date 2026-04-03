<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
  >
    <template #actions>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-import"
        @click="showImportDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.import') }}</span>
      </UButton>
      <UButton
        color="primary"
        icon="i-lucide-plus"
        data-tour="settings-identities-create"
        @click="showCreateDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.create') }}</span>
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

      <!-- Identities list -->
      <div
        v-else-if="identities.length"
        class="space-y-3"
      >
        <div
          v-for="identity in identities"
          :key="identity.id"
          class="p-3 rounded-lg border border-default"
        >
          <UCollapsible
            :open="expandedIdentity === identity.id"
            :unmount-on-hide="false"
            @update:open="(val: boolean) => onToggleIdentity(identity.id, val)"
          >
            <div class="flex items-center justify-between cursor-pointer">
              <div class="flex items-center gap-2 flex-1 min-w-0">
                <UIcon
                  name="i-lucide-chevron-right"
                  class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
                  :class="{ 'rotate-90': expandedIdentity === identity.id }"
                />
                <UiAvatar
                  :src="identity.avatar"
                  :seed="identity.publicKey"
                  avatar-style="toon-head"
                  size="sm"
                />
                <span class="font-medium truncate">{{ identity.label }}</span>
              </div>

              <div class="shrink-0 ml-4" @click.stop>
                <!-- Large screens: inline buttons -->
                <div class="hidden @md:flex items-center gap-1">
                  <UButton variant="ghost" icon="i-lucide-qr-code" :title="t('actions.shareQr')" @click="onShareQr(identity)" />
                  <UButton variant="ghost" icon="i-lucide-copy" :title="t('actions.copyDid')" @click="copyDid(identity.did)" />
                  <UButton variant="ghost" icon="i-lucide-download" :title="t('actions.export')" @click="onExport(identity)" />
                  <UButton variant="ghost" icon="i-lucide-pencil" :title="t('actions.edit')" @click="openRenameDialog(identity)" />
                  <UButton variant="ghost" color="error" icon="i-lucide-trash-2" :title="t('actions.delete')" @click="prepareDelete(identity)" />
                </div>
                <!-- Small screens: dropdown menu -->
                <UDropdownMenu
                  class="@md:hidden"
                  :items="[
                    [
                      { label: t('actions.shareQr'), icon: 'i-lucide-qr-code', onSelect: () => onShareQr(identity) },
                      { label: t('actions.copyDid'), icon: 'i-lucide-copy', onSelect: () => copyDid(identity.did) },
                      { label: t('actions.export'), icon: 'i-lucide-download', onSelect: () => onExport(identity) },
                      { label: t('actions.edit'), icon: 'i-lucide-pencil', onSelect: () => openRenameDialog(identity) },
                    ],
                    [
                      { label: t('actions.delete'), icon: 'i-lucide-trash-2', color: 'error' as const, onSelect: () => prepareDelete(identity) },
                    ],
                  ]"
                >
                  <UButton variant="ghost" icon="i-lucide-ellipsis-vertical" color="neutral" />
                </UDropdownMenu>
              </div>
            </div>
            <template #content>
              <div class="mt-3 pt-3 border-t border-default space-y-3">
                <!-- DID Key -->
                <div class="flex items-center gap-2">
                  <code class="text-xs text-muted truncate flex-1 min-w-0">{{ identity.did }}</code>
                  <UButton
                    variant="ghost"
                    icon="i-lucide-copy"
                    size="xs"
                    :title="t('actions.copyDid')"
                    @click="copyDid(identity.did)"
                  />
                </div>

                <!-- Claims -->
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <span class="text-sm font-medium">{{
                    t('claims.title')
                  }}</span>
                  <UButton
                    variant="outline"
                    icon="i-lucide-plus"
                    @click="openAddClaim(identity.id)"
                  >
                    {{ t('claims.add') }}
                  </UButton>
                </div>

                <div
                  v-if="identityClaims[identity.id]?.length"
                  class="space-y-1"
                >
                  <div
                    v-for="claim in identityClaims[identity.id]"
                    :key="claim.id"
                    class="flex flex-wrap items-center justify-between gap-2 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
                  >
                    <div class="min-w-0 flex-1">
                      <span class="text-xs font-medium text-muted">{{
                        claim.type
                      }}</span>
                      <p class="text-sm truncate">{{ claim.value }}</p>
                    </div>
                    <div class="flex gap-1 shrink-0">
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
                        @click="deleteClaimAsync(claim.id, identity.id)"
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
              </div>
            </template>
          </UCollapsible>
        </div>
      </div>

      <!-- Empty state -->
      <HaexSystemSettingsLayoutEmpty
        v-else
        :message="t('list.empty')"
        icon="i-lucide-fingerprint"
      />
    <!-- Create Identity Dialog -->
    <UiDrawerModal
      v-model:open="showCreateDialog"
      :title="t('create.title')"
      :description="t('create.description')"
    >
      <template #body>
        <div class="space-y-4">
          <div class="flex justify-center">
            <UiAvatarPicker
              v-model="createAvatar"
              :seed="createLabel || 'new'"
              avatar-style="toon-head"
              size="xl"
            />
          </div>

          <UiInput
            v-model="createLabel"
            :label="t('create.labelField')"
            :placeholder="t('create.labelPlaceholder')"
          />

          <USeparator :label="t('create.syncCredentials')" />

          <UiInput
            v-model="createClaims.email"
            label="Email"
            placeholder="user@example.com"
            leading-icon="i-lucide-mail"
            type="email"
            required
            :custom-validators="[emailValidator]"
            check
          />

          <UCheckbox
            v-model="useVaultPasswordForIdentity"
            :label="t('create.useVaultPassword')"
          />

          <template v-if="!useVaultPasswordForIdentity">
            <UiInputPassword
              v-model="createIdentityPassword"
              :label="t('create.identityPassword')"
              :description="t('create.identityPasswordDescription')"
              leading-icon="i-lucide-lock"
            />
            <UiInputPassword
              v-model="createIdentityPasswordConfirm"
              :label="t('create.identityPasswordConfirm')"
              leading-icon="i-lucide-lock"
            />
            <p
              v-if="
                createIdentityPasswordConfirm &&
                createIdentityPassword !== createIdentityPasswordConfirm
              "
              class="text-sm text-error -mt-3"
            >
              {{ t('create.passwordMismatch') }}
            </p>
          </template>

          <USeparator :label="t('create.claimsOptional')" />

          <UiInput
            v-model="createClaims.name"
            label="Name"
            placeholder="Max Mustermann"
            leading-icon="i-lucide-user"
          />
          <UiInput
            v-model="createClaims.phone"
            :label="t('claims.phone')"
            placeholder="+49 123 456789"
            leading-icon="i-lucide-phone"
          />
          <UiInput
            v-model="createClaims.address"
            :label="t('claims.address')"
            placeholder="Musterstraße 1, 12345 Berlin"
            leading-icon="i-lucide-map-pin"
          />
        </div>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showCreateDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-plus"
            :loading="isCreating"
            :disabled="!canCreateIdentity"
            @click="onCreateAsync"
          >
            {{ t('actions.create') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Import Identity Dialog -->
    <UiDrawerModal
      v-model:open="showImportDialog"
      :title="t('import.title')"
      :description="t('import.description')"
    >
      <template #body>
        <!-- Step 1: Load data -->
        <template v-if="!importParsed">
          <div class="space-y-4">
            <UButton
              color="neutral"
              variant="outline"
              icon="i-lucide-file-up"
              block
              @click="onSelectImportFileAsync"
            >
              {{ t('import.selectFile') }}
            </UButton>

            <USeparator :label="t('import.orPaste')" />

            <UiTextarea
              v-model="importJson"
              :label="t('import.jsonLabel')"
              :placeholder="t('import.jsonPlaceholder')"
              :rows="6"
            />
          </div>
        </template>

        <!-- Step 2: Preview & select -->
        <template v-else>
          <div class="space-y-4">
            <!-- Identity info -->
            <div class="flex items-center gap-3 p-3 rounded-lg border border-default">
              <UiAvatar
                v-if="importParsed.avatar"
                :src="importParsed.avatar"
                :seed="importParsed.publicKey"
                avatar-style="toon-head"
                size="sm"
              />
              <div class="min-w-0 flex-1">
                <p class="font-medium truncate">{{ importParsed.label || importParsed.publicKey.slice(0, 20) + '...' }}</p>
                <p class="text-xs text-muted truncate">{{ importParsed.publicKey }}</p>
              </div>
              <UBadge
                :color="importParsed.privateKey ? 'primary' : 'neutral'"
                variant="subtle"
                size="sm"
              >
                {{ importParsed.privateKey ? t('import.typeIdentity') : t('import.typeContact') }}
              </UBadge>
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
                avatar-style="toon-head"
                size="sm"
              />
              <span class="text-sm">{{ t('import.includeAvatar') }}</span>
            </div>

            <!-- Claims -->
            <div
              v-if="importParsed.claims.length"
              class="space-y-2"
            >
              <span class="text-sm font-medium">{{ t('import.selectClaims') }}</span>
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
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="importParsed ? (importParsed = null) : (showImportDialog = false)"
          >
            {{ importParsed ? t('actions.back') : t('actions.cancel') }}
          </UButton>
          <UiButton
            v-if="!importParsed"
            icon="i-lucide-arrow-right"
            :disabled="!importJson.trim()"
            @click="onParseImportAsync"
          >
            {{ t('import.preview') }}
          </UiButton>
          <UiButton
            v-else
            icon="i-lucide-import"
            :loading="isImporting"
            @click="onImportAsync"
          >
            {{ t('actions.import') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Export Identity Dialog -->
    <UiDrawerModal
      v-model:open="showExportDialog"
      :title="t('export.title')"
      :description="t('export.description')"
    >
      <template #body>
        <!-- Avatar -->
        <div
          v-if="exportTarget?.avatar"
          class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
        >
          <UCheckbox v-model="exportIncludeAvatar" />
          <UiAvatar
            :src="exportTarget.avatar"
            :seed="exportTarget.publicKey"
            avatar-style="toon-head"
            size="sm"
          />
          <span class="text-sm">{{ t('export.includeAvatar') }}</span>
        </div>

        <!-- Claims selection -->
        <div
          v-if="exportClaims.length"
          class="space-y-2"
        >
          <span class="text-sm font-medium">{{ t('export.selectClaims') }}</span>
          <div
            v-for="claim in exportClaims"
            :key="claim.id"
            class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
          >
            <UCheckbox
              :model-value="exportSelectedClaimIds.has(claim.id)"
              @update:model-value="toggleExportClaim(claim.id)"
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
          {{ t('export.noClaims') }}
        </p>

        <!-- Private key (hidden behind collapsible) -->
        <UCollapsible class="mt-4">
          <div class="flex items-center gap-2 cursor-pointer text-sm text-muted">
            <UIcon
              name="i-lucide-chevron-right"
              class="w-4 h-4 shrink-0 transition-transform duration-200"
              :class="{ 'rotate-90': exportIncludePrivateKey }"
            />
            <UIcon name="i-lucide-shield-alert" class="w-4 h-4 text-red-500" />
            <span>{{ t('export.advancedSection') }}</span>
          </div>
          <template #content>
            <div class="mt-2 p-3 rounded-lg border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-950/30">
              <div class="flex items-start gap-3">
                <UCheckbox
                  v-model="exportIncludePrivateKey"
                  color="error"
                />
                <div>
                  <span class="text-sm font-medium text-red-700 dark:text-red-400">{{ t('export.includePrivateKey') }}</span>
                  <p class="text-xs text-red-600 dark:text-red-500 mt-1">
                    {{ t('export.privateKeyWarning') }}
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
            @click="showExportDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-download"
            :loading="isExporting"
            @click="onExportFileAsync"
          >
            {{ t('export.saveFile') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Private Key Export Confirmation -->
    <UiDialogConfirm
      v-model:open="showPrivateKeyConfirm"
      :title="t('export.confirmPrivateKey.title')"
      :description="t('export.confirmPrivateKey.description')"
      @confirm="onConfirmExportWithPrivateKeyAsync"
    />

    <!-- Edit Identity Dialog -->
    <UiDrawerModal
      v-model:open="showRenameDialog"
      :title="t('edit.title')"
    >
      <template #body>
        <div class="space-y-4">
          <div class="flex justify-center">
            <UiAvatarPicker
              :model-value="renameTarget?.avatar"
              :seed="renameTarget?.publicKey"
              avatar-style="toon-head"
              size="xl"
              @update:model-value="(val) => renameTarget && updateAvatarAsync(renameTarget.publicKey, val)"
            />
          </div>

          <UiInput
            v-model="renameLabel"
            :label="t('edit.labelField')"
            @keydown.enter.prevent="onRenameAsync"
          />

          <USeparator :label="t('edit.changePassword')" />

          <UiInputPassword
            v-model="editIdentityPassword"
            :label="t('create.identityPassword')"
            :description="t('edit.passwordOptional')"
            leading-icon="i-lucide-lock"
          />
          <UiInputPassword
            v-if="editIdentityPassword"
            v-model="editIdentityPasswordConfirm"
            :label="t('create.identityPasswordConfirm')"
            leading-icon="i-lucide-lock"
          />
          <p
            v-if="
              editIdentityPasswordConfirm &&
              editIdentityPassword !== editIdentityPasswordConfirm
            "
            class="text-sm text-error -mt-3"
          >
            {{ t('create.passwordMismatch') }}
          </p>
        </div>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showRenameDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-check"
            :loading="isRenaming"
            :disabled="!canSaveEdit"
            @click="onRenameAsync"
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
          <USelectMenu
            v-if="!editingClaim"
            v-model="claimType"
            :items="claimTypeOptions"
            value-key="value"
            :label="t('claims.type')"
            class="min-w-48"
          />
          <UiInput
            v-if="claimType === 'custom' && !editingClaim"
            v-model="claimCustomType"
            :label="t('claims.customType')"
            placeholder="z.B. phone, company"
          />
          <UiInput
            v-model="claimValue"
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
      :confirm-label="t('delete.confirmLabel')"
      confirm-icon="i-lucide-trash-2"
      @confirm="onConfirmDeleteAsync"
    >
      <div v-if="affectedAdminSpaces.length > 0" class="mt-4 space-y-2">
        <p class="text-sm font-medium text-highlighted">
          {{ t('delete.adminSpacesWarning', { count: affectedAdminSpaces.length }) }}
        </p>
        <ul class="list-disc list-inside text-sm text-muted">
          <li v-for="space in affectedAdminSpaces" :key="space.id" class="font-medium">
            {{ space.name }}
          </li>
        </ul>
      </div>
      <div v-if="affectedMemberSpaces.length > 0" class="mt-3 space-y-2">
        <p class="text-sm text-muted">
          {{ t('delete.memberSpacesInfo', { count: affectedMemberSpaces.length }) }}
        </p>
      </div>
    </UiDialogConfirm>

    <!-- Share Identity QR Dialog -->
    <ShareIdentityDialog
      v-model:open="showShareQrDialog"
      :pre-selected-identity-id="shareQrIdentityId"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { save } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import type { SelectHaexIdentities } from '~/database/schemas'
import { useUpdateIdentityPassword } from '@/composables/useUpdateIdentityPassword'
import ShareIdentityDialog from './contacts/ShareIdentityDialog.vue'

const { t } = useI18n()
const { add } = useToast()

const identityStore = useIdentityStore()
const { updatePasswordAsync } = useUpdateIdentityPassword()
const { ownIdentities: identities } = storeToRefs(identityStore)
const { currentVaultPassword } = storeToRefs(useVaultStore())

const isLoading = ref(false)
const isCreating = ref(false)
const isRenaming = ref(false)
const isImporting = ref(false)

const showCreateDialog = ref(false)
const showRenameDialog = ref(false)
const showDeleteConfirm = ref(false)
const showImportDialog = ref(false)
const showExportDialog = ref(false)
const showShareQrDialog = ref(false)
const shareQrIdentityId = ref('')

const createLabel = ref('')
const createAvatar = ref<string | null>(null)
const useVaultPasswordForIdentity = ref(true)
const createIdentityPassword = ref('')
const createIdentityPasswordConfirm = ref('')
const createClaims = reactive({
  email: '',
  name: '',
  phone: '',
  address: '',
})

const effectiveCreatePassword = computed(() =>
  useVaultPasswordForIdentity.value
    ? (currentVaultPassword.value ?? '')
    : createIdentityPassword.value,
)

const isValidEmail = (email: string): boolean => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)
const emailValidator = (value: unknown): string | null => {
  const v = String(value ?? '').trim()
  if (!v) return null
  return isValidEmail(v) ? null : t('create.invalidEmail')
}

const canCreateIdentity = computed(() => {
  if (!createLabel.value.trim() || !isValidEmail(createClaims.email)) return false
  if (useVaultPasswordForIdentity.value) return !!currentVaultPassword.value
  return (
    createIdentityPassword.value.length >= 8 &&
    createIdentityPassword.value === createIdentityPasswordConfirm.value
  )
})
const renameLabel = ref('')
const renameTarget = ref<SelectHaexIdentities | null>(null)
const editIdentityPassword = ref('')
const editIdentityPasswordConfirm = ref('')

const canSaveEdit = computed(() => {
  if (!renameLabel.value.trim()) return false
  if (editIdentityPassword.value) {
    return (
      editIdentityPassword.value.length >= 8 &&
      editIdentityPassword.value === editIdentityPasswordConfirm.value
    )
  }
  return true
})
const deleteTarget = ref<SelectHaexIdentities | null>(null)
const affectedAdminSpaces = ref<{ id: string; name: string }[]>([])
const affectedMemberSpaces = ref<{ id: string; name: string }[]>([])
const importJson = ref('')
const importParsed = ref<{
  label: string
  publicKey: string
  did?: string
  privateKey?: string
  avatar?: string | null
  claims: { type: string; value: string }[]
} | null>(null)
const importSelectedClaimIndices = ref(new Set<number>())
const importIncludeAvatar = ref(true)
const exportTarget = ref<SelectHaexIdentities | null>(null)
const exportClaims = ref<{ id: string; type: string; value: string }[]>([])
const exportSelectedClaimIds = ref(new Set<string>())
const exportIncludeAvatar = ref(true)
const exportIncludePrivateKey = ref(false)
const isExporting = ref(false)
const showPrivateKeyConfirm = ref(false)

onMounted(async () => {
  isLoading.value = true
  try {
    await identityStore.loadIdentitiesAsync()
  } finally {
    isLoading.value = false
  }
})

const updateAvatarAsync = async (identityId: string, avatar: string | null) => {
  await identityStore.updateAvatarAsync(identityId, avatar)
}

const onCreateAsync = async () => {
  if (!createLabel.value.trim()) return

  isCreating.value = true
  try {
    const identity = await identityStore.createIdentityAsync(
      createLabel.value.trim(),
    )

    // Save avatar: use uploaded image, or generate toon-head from publicKey
    if (createAvatar.value) {
      await identityStore.updateAvatarAsync(identity.id, createAvatar.value)
    } else {
      const { createAvatar: createDicebear } = await import('@dicebear/core')
      const toonHead = await import('@dicebear/toon-head')
      const svg = createDicebear(toonHead, { seed: identity.publicKey }).toDataUri()
      await identityStore.updateAvatarAsync(identity.id, svg)
    }

    // Store identity password for use when connecting to a sync backend
    if (effectiveCreatePassword.value) {
      identityStore.setIdentityPassword(
        identity.id,
        effectiveCreatePassword.value,
      )
    }

    // Save non-empty claims
    const claimEntries = Object.entries(createClaims).filter(([, value]) =>
      value.trim(),
    )
    for (const [type, value] of claimEntries) {
      await identityStore.addClaimAsync(identity.id, type, value.trim())
    }

    add({ title: t('success.created'), color: 'success' })
    showCreateDialog.value = false
    createLabel.value = ''
    createAvatar.value = null
    useVaultPasswordForIdentity.value = true
    createIdentityPassword.value = ''
    createIdentityPasswordConfirm.value = ''
    createClaims.email = ''
    createClaims.name = ''
    createClaims.phone = ''
    createClaims.address = ''
  } catch (error) {
    console.error('Failed to create identity:', error)
    add({
      title: t('errors.createFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isCreating.value = false
  }
}

const onSelectImportFileAsync = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const { readFile } = await import('@tauri-apps/plugin-fs')

    const filePath = await open({
      title: t('import.title'),
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

const onParseImportAsync = () => {
  if (!importJson.value.trim()) return

  let parsed: Record<string, unknown>
  try {
    parsed = JSON.parse(importJson.value)
  } catch {
    add({ title: t('errors.invalidJson'), color: 'error' })
    return
  }

  if (!parsed.publicKey) {
    add({ title: t('errors.invalidIdentityData'), color: 'error' })
    return
  }

  const claims = Array.isArray(parsed.claims)
    ? (parsed.claims as { type: string; value: string }[])
    : []

  importParsed.value = {
    label: (parsed.label as string) || '',
    publicKey: parsed.publicKey as string,
    did: parsed.did as string | undefined,
    privateKey: parsed.privateKey as string | undefined,
    avatar: typeof parsed.avatar === 'string' ? parsed.avatar : null,
    claims,
  }

  // Select all claims by default
  importSelectedClaimIndices.value = new Set(claims.map((_, i) => i))
  importIncludeAvatar.value = !!importParsed.value.avatar
}

const onImportAsync = async () => {
  if (!importParsed.value) return

  isImporting.value = true
  try {
    const data = importParsed.value
    const selectedClaims = data.claims.filter((_, i) => importSelectedClaimIndices.value.has(i))
    const avatar = importIncludeAvatar.value ? data.avatar : null

    if (data.privateKey && data.did) {
      // Full identity import (backup restore)
      await identityStore.importIdentityAsync({
        did: data.did,
        label: data.label,
        publicKey: data.publicKey,
        privateKey: data.privateKey,
        avatar,
        claims: selectedClaims,
      })
      add({ title: t('success.imported'), color: 'success' })
    } else {
      // No private key — import as contact
      const contact = await identityStore.addContactWithClaimsAsync(
        data.label || `Imported ${data.publicKey.slice(0, 16)}...`,
        data.publicKey,
        selectedClaims,
      )
      if (avatar) {
        await identityStore.updateContactAsync(contact.id, { avatar })
      }
      add({ title: t('success.importedAsContact'), color: 'success' })
    }

    showImportDialog.value = false
    importJson.value = ''
    importParsed.value = null
  } catch (error) {
    console.error('Failed to import:', error)
    add({
      title: t('errors.importFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isImporting.value = false
  }
}

const onShareQr = (identity: SelectHaexIdentities) => {
  shareQrIdentityId.value = identity.id
  showShareQrDialog.value = true
}

const onExport = async (identity: SelectHaexIdentities) => {
  exportTarget.value = identity
  exportIncludePrivateKey.value = false
  exportIncludeAvatar.value = !!identity.avatar

  const claims = await identityStore.getClaimsAsync(identity.id)
  exportClaims.value = claims.map(c => ({ id: c.id, type: c.type, value: c.value }))
  exportSelectedClaimIds.value = new Set(exportClaims.value.map(c => c.id))

  showExportDialog.value = true
}

const toggleExportClaim = (claimId: string) => {
  if (exportSelectedClaimIds.value.has(claimId)) {
    exportSelectedClaimIds.value.delete(claimId)
  } else {
    exportSelectedClaimIds.value.add(claimId)
  }
}

const onExportFileAsync = async () => {
  if (!exportTarget.value) return

  if (exportIncludePrivateKey.value) {
    showPrivateKeyConfirm.value = true
    return
  }

  await doExportAsync()
}

const onConfirmExportWithPrivateKeyAsync = async () => {
  showPrivateKeyConfirm.value = false
  await doExportAsync()
}

const doExportAsync = async () => {
  if (!exportTarget.value) return

  isExporting.value = true
  try {
    const identity = exportTarget.value
    const selectedClaims = exportClaims.value
      .filter(c => exportSelectedClaimIds.value.has(c.id))
      .map(c => ({ type: c.type, value: c.value }))

    const payload: Record<string, unknown> = {
      did: identity.did,
      label: identity.label,
      publicKey: identity.publicKey,
      claims: selectedClaims,
    }

    if (exportIncludeAvatar.value && identity.avatar) {
      payload.avatar = identity.avatar
    }

    if (exportIncludePrivateKey.value) {
      payload.privateKey = identity.privateKey
    }

    const json = JSON.stringify(payload, null, 2)
    const data = new TextEncoder().encode(json)

    const filePath = await save({
      title: t('export.title'),
      defaultPath: `${identity.label.replace(/[^a-zA-Z0-9_-]/g, '_')}.identity.json`,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    })
    if (!filePath) return

    await writeFile(filePath, data)
    add({ title: t('success.exported'), color: 'success' })
    showExportDialog.value = false
  } catch (error) {
    console.error('Failed to export identity:', error)
    add({
      title: t('errors.exportFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isExporting.value = false
  }
}

const openRenameDialog = (identity: SelectHaexIdentities) => {
  renameTarget.value = identity
  renameLabel.value = identity.label
  editIdentityPassword.value = ''
  editIdentityPasswordConfirm.value = ''
  showRenameDialog.value = true
}

const onRenameAsync = async () => {
  if (!renameTarget.value || !renameLabel.value.trim()) return

  isRenaming.value = true
  try {
    await identityStore.updateLabelAsync(
      renameTarget.value.id,
      renameLabel.value.trim(),
    )

    if (editIdentityPassword.value) {
      const ok = await updatePasswordAsync(
        renameTarget.value.id,
        editIdentityPassword.value,
      )
      if (!ok) {
        add({ title: t('errors.passwordUpdateFailed'), color: 'error' })
        return
      }
    }

    add({ title: t('success.saved'), color: 'success' })
    showRenameDialog.value = false
    editIdentityPassword.value = ''
    editIdentityPasswordConfirm.value = ''
  } catch (error) {
    console.error('Failed to edit identity:', error)
    add({
      title: t('errors.editFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isRenaming.value = false
  }
}

const prepareDelete = async (identity: SelectHaexIdentities) => {
  deleteTarget.value = identity
  const affected = await identityStore.getAffectedSpacesAsync(identity.id)
  affectedAdminSpaces.value = affected.adminSpaces
  affectedMemberSpaces.value = affected.memberSpaces
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
    console.error('Failed to delete identity:', error)
    add({
      title: t('errors.deleteFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const copyDid = async (did: string) => {
  try {
    await navigator.clipboard.writeText(did)
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
const expandedIdentity = ref<string | null>(null)
const identityClaims = ref<
  Record<string, { id: string; type: string; value: string }[]>
>({})
const showClaimDialog = ref(false)
const claimType = ref('email')
const claimCustomType = ref('')
const claimValue = ref('')
const editingClaim = ref<{
  id: string
  identityId: string
  type: string
} | null>(null)
const claimTargetIdentityId = ref<string | null>(null)

const claimTypeOptions = computed(() => [
  { label: 'Email', value: 'email' },
  { label: 'Name', value: 'name' },
  { label: t('claims.phone'), value: 'phone' },
  { label: t('claims.address'), value: 'address' },
  { label: t('claims.custom'), value: 'custom' },
])

const claimValuePlaceholder = computed(() => {
  if (editingClaim.value) return ''
  if (claimType.value === 'email') return 'user@example.com'
  if (claimType.value === 'name') return 'Max Mustermann'
  if (claimType.value === 'phone') return '+49 123 456789'
  if (claimType.value === 'address') return 'Musterstraße 1, 12345 Berlin'
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

const onToggleIdentity = async (identityId: string, open: boolean) => {
  if (!open) {
    expandedIdentity.value = null
    return
  }
  expandedIdentity.value = identityId
  await loadClaimsAsync(identityId)
}

const loadClaimsAsync = async (identityId: string) => {
  const claims = await identityStore.getClaimsAsync(identityId)
  identityClaims.value[identityId] = claims.map((c) => ({
    id: c.id,
    type: c.type,
    value: c.value,
  }))
}

const openAddClaim = (identityId: string) => {
  claimTargetIdentityId.value = identityId
  editingClaim.value = null
  // Pre-select first available (non-disabled) type
  const firstAvailable = claimTypeOptions.value.find((o) => !o.disabled)
  claimType.value = firstAvailable?.value ?? 'custom'
  claimCustomType.value = ''
  claimValue.value = ''
  showClaimDialog.value = true
}

const openEditClaim = (claim: { id: string; type: string; value: string }) => {
  editingClaim.value = {
    id: claim.id,
    identityId: expandedIdentity.value!,
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
      await loadClaimsAsync(editingClaim.value.identityId)
      add({ title: t('claims.updated'), color: 'success' })
    } else {
      const type =
        claimType.value === 'custom'
          ? claimCustomType.value.trim()
          : claimType.value
      await identityStore.addClaimAsync(
        claimTargetIdentityId.value!,
        type,
        claimValue.value.trim(),
      )
      await loadClaimsAsync(claimTargetIdentityId.value!)
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

const deleteClaimAsync = async (claimId: string, identityId: string) => {
  try {
    await identityStore.deleteClaimAsync(claimId)
    await loadClaimsAsync(identityId)
    add({ title: t('claims.deleted'), color: 'success' })
  } catch (error) {
    console.error('Failed to delete claim:', error)
    add({ title: t('claims.deleteFailed'), color: 'error' })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Identitäten
  description: Verwalte deine kryptographischen Identitäten (did:key)
  avatar:
    hint: Klicke auf das Bild, um ein Profilbild hochzuladen
  list:
    title: Deine Identitäten
    description: Jede Identität ist ein einzigartiges Schlüsselpaar für die Nutzung in Spaces
    empty: Keine Identitäten vorhanden
    created: Erstellt
  create:
    title: Identität erstellen
    description: Erstelle eine neue kryptographische Identität. Jede Identität hat ihren eigenen Schlüssel und kann unabhängig in verschiedenen Spaces genutzt werden.
    labelField: Name
    labelPlaceholder: z.B. Persönlich, Arbeit, Anonym
    syncCredentials: Sync-Zugangsdaten
    useVaultPassword: Gleiches Passwort wie Vault verwenden
    identityPassword: Identity-Passwort
    identityPasswordDescription: Dieses Passwort schützt deinen privaten Schlüssel auf dem Sync-Server. Merke es dir gut – es wird für die Wiederherstellung benötigt.
    identityPasswordConfirm: Identity-Passwort bestätigen
    passwordMismatch: Passwörter stimmen nicht überein
    invalidEmail: Bitte eine gültige E-Mail-Adresse eingeben
    claimsOptional: Weitere Angaben (optional)
  import:
    title: Identität importieren
    description: Importiere eine Identität oder einen Kontakt aus einer JSON-Datei. Enthält die Datei einen privaten Schlüssel, wird sie als Identität importiert — andernfalls als Kontakt.
    selectFile: JSON-Datei auswählen
    orPaste: oder einfügen
    jsonLabel: Identitäts-JSON
    jsonPlaceholder: Exportiertes Identitäts-JSON hier einfügen
    preview: Vorschau
    typeIdentity: Identität
    typeContact: Kontakt
    includeAvatar: Profilbild übernehmen
    selectClaims: Claims zum Importieren auswählen
  export:
    title: Identität exportieren
    description: Wähle aus, welche Daten in die exportierte Datei aufgenommen werden sollen.
    selectClaims: Claims zum Exportieren auswählen
    noClaims: Keine Claims vorhanden. Nur der Public Key wird exportiert.
    includeAvatar: Profilbild einschließen
    advancedSection: Backup & Wiederherstellung
    includePrivateKey: Privaten Schlüssel einschließen
    privateKeyWarning: 'Nur für persönliche Backups! Teile diese Datei NIEMALS mit anderen Personen. Wer deinen privaten Schlüssel besitzt, kann deine Identität vollständig übernehmen.'
    confirmPrivateKey:
      title: Privaten Schlüssel wirklich exportieren?
      description: 'Der private Schlüssel gibt volle Kontrolle über deine Identität. Exportiere ihn nur, wenn du ein persönliches Backup erstellen möchtest. Teile diese Datei NIEMALS mit anderen.'
    saveFile: Als Datei speichern
  edit:
    title: Identität bearbeiten
    labelField: Name
    changePassword: Passwort ändern (optional)
    passwordOptional: Leer lassen, um das Passwort beizubehalten
  delete:
    title: Identität löschen
    description: Möchtest du diese Identität wirklich löschen? Diese Aktion kann nicht rückgängig gemacht werden.
    confirmLabel: Endgültig löschen
    adminSpacesWarning: 'Diese Identität ist Admin von {count} Space(s), die unwiderruflich gelöscht werden:'
    memberSpacesInfo: 'Du wirst außerdem aus {count} Space(s) entfernt, in denen du Mitglied bist.'
  claims:
    title: Claims
    add: Hinzufügen
    addTitle: Claim hinzufügen
    editTitle: Claim bearbeiten
    type: Typ
    customType: Benutzerdefinierter Typ
    phone: Telefon
    address: Adresse
    custom: Benutzerdefiniert
    value: Wert
    empty: Keine Claims vorhanden. Füge Email, Name oder andere Daten hinzu.
    added: Claim hinzugefügt
    updated: Claim aktualisiert
    deleted: Claim gelöscht
    saveFailed: Claim konnte nicht gespeichert werden
    deleteFailed: Claim konnte nicht gelöscht werden
  actions:
    create: Erstellen
    import: Importieren
    export: Exportieren
    cancel: Abbrechen
    back: Zurück
    close: Schließen
    save: Speichern
    edit: Bearbeiten
    delete: Löschen
    shareQr: Als QR-Code teilen
    copyDid: DID kopieren
    toggleClaims: Claims anzeigen/verbergen
  success:
    created: Identität erstellt
    imported: Identität importiert
    importedAsContact: Als Kontakt hinzugefügt
    exported: Identität exportiert
    saved: Identität gespeichert
    deleted: Identität gelöscht
    copied: Kopiert
  errors:
    createFailed: Identität konnte nicht erstellt werden
    importFailed: Import fehlgeschlagen
    exportFailed: Export fehlgeschlagen
    invalidJson: Ungültiges JSON-Format
    invalidIdentityData: Unvollständige Daten (mindestens publicKey erforderlich)
    editFailed: Speichern fehlgeschlagen
    passwordUpdateFailed: Passwort konnte nicht auf dem Server aktualisiert werden
    deleteFailed: Löschen fehlgeschlagen
    copyFailed: Kopieren fehlgeschlagen
en:
  title: Identities
  description: Manage your cryptographic identities (did:key)
  avatar:
    hint: Click the image to upload a profile picture
  list:
    title: Your Identities
    description: Each identity is a unique keypair for use in Spaces
    empty: No identities found
    created: Created
  create:
    title: Create Identity
    description: Create a new cryptographic identity. Each identity has its own key and can be used independently in different Spaces.
    labelField: Name
    labelPlaceholder: e.g. Personal, Work, Anonymous
    syncCredentials: Sync credentials
    useVaultPassword: Use same password as vault
    identityPassword: Identity password
    identityPasswordDescription: This password protects your private key on the sync server. Remember it well — it is required for account recovery.
    identityPasswordConfirm: Confirm identity password
    passwordMismatch: Passwords do not match
    invalidEmail: Please enter a valid email address
    claimsOptional: Additional info (optional)
  import:
    title: Import Identity
    description: Import an identity or contact from a JSON file. If the file contains a private key, it will be imported as an identity — otherwise as a contact.
    selectFile: Select JSON file
    orPaste: or paste
    jsonLabel: Identity JSON
    jsonPlaceholder: Paste exported identity JSON here
    preview: Preview
    typeIdentity: Identity
    typeContact: Contact
    includeAvatar: Include profile picture
    selectClaims: Select claims to import
  export:
    title: Export Identity
    description: Choose which data to include in the exported file.
    selectClaims: Select claims to export
    noClaims: No claims available. Only the public key will be exported.
    includeAvatar: Include profile picture
    advancedSection: Backup & Recovery
    includePrivateKey: Include private key
    privateKeyWarning: 'For personal backups only! NEVER share this file with others. Anyone with your private key can fully impersonate your identity.'
    confirmPrivateKey:
      title: Really export private key?
      description: 'The private key gives full control over your identity. Only export it if you want to create a personal backup. NEVER share this file with others.'
    saveFile: Save as file
  edit:
    title: Edit Identity
    labelField: Name
    changePassword: Change password (optional)
    passwordOptional: Leave empty to keep the current password
  delete:
    title: Delete Identity
    description: Do you really want to delete this identity? This action cannot be undone.
    confirmLabel: Delete permanently
    adminSpacesWarning: 'This identity is admin of {count} space(s) that will be permanently deleted:'
    memberSpacesInfo: 'You will also be removed from {count} space(s) where you are a member.'
  claims:
    title: Claims
    add: Add
    addTitle: Add Claim
    editTitle: Edit Claim
    type: Type
    customType: Custom Type
    phone: Phone
    address: Address
    custom: Custom
    value: Value
    empty: No claims yet. Add email, name or other data.
    added: Claim added
    updated: Claim updated
    deleted: Claim deleted
    saveFailed: Failed to save claim
    deleteFailed: Failed to delete claim
  actions:
    create: Create
    import: Import
    export: Export
    cancel: Cancel
    back: Back
    close: Close
    save: Save
    edit: Edit
    delete: Delete
    shareQr: Share as QR code
    copyDid: Copy DID
    toggleClaims: Show/hide claims
  success:
    created: Identity created
    imported: Identity imported
    importedAsContact: Added as contact
    exported: Identity exported
    saved: Identity saved
    deleted: Identity deleted
    copied: Copied
  errors:
    createFailed: Failed to create identity
    importFailed: Failed to import identity
    exportFailed: Failed to export identity
    invalidJson: Invalid JSON format
    invalidIdentityData: Incomplete data (at least publicKey is required)
    editFailed: Failed to save identity
    passwordUpdateFailed: Failed to update password on the server
    deleteFailed: Failed to delete identity
    copyFailed: Failed to copy
</i18n>
