<template>
  <form
    class="h-full flex flex-col overflow-hidden"
    @submit.prevent="onSave"
  >
    <HaexSystemPasswordsEditorHeader
      :title="form.title"
      :color="form.color"
      :is-creating="isCreating"
      :is-editing="isEditing"
      :is-current-item-in-trash="isCurrentItemInTrash"
      :saving="saving"
      :icon-descriptor="iconDescriptor"
      :binary-icon-src="binaryIconSrc"
      :icon-background-style="iconBackgroundStyle"
      @back="onBack"
      @restore="onRestore"
      @start-edit="startEdit"
      @request-delete="showDeleteDialog = true"
    />

    <UTabs
      v-model="activeTab"
      :items="tabItems"
      class="flex-1 min-h-0 flex flex-col"
      :ui="{
        list: 'shrink-0 mx-3 my-2',
        content: 'flex-1 min-h-0 overflow-y-auto',
      }"
    >
      <!-- Details -->
      <template #details>
        <HaexSystemPasswordsEditorDetails
          :form="form"
          :errors="errors"
          :is-editing="isEditing"
          :is-expired="isExpired"
          :otp-code="otpCode"
          :otp-formatted="otpFormatted"
          :otp-remaining="otpRemaining"
          :otp-dash-array="otpDashArray"
          :copied-otp="copiedOtp"
          :copy-otp="copyOtp"
          :otp-algorithms="otpAlgorithms"
          @open-generator="generatorOpen = true"
        />
      </template>

      <!-- Extra -->
      <template #extra>
        <HaexSystemPasswordsEditorExtra
          ref="extraRef"
          :visible-key-values="visibleKeyValues"
          :current-selected-kv="currentSelectedKv"
          :current-kv-value="currentKvValue"
          :kv-copied-item="kvCopiedItem"
          :attachments="attachments"
          :attachments-to-add="attachmentsToAdd"
          :attachments-to-delete="attachmentsToDelete"
          :item-id="selectedItem?.id"
          :autofill-aliases="form.autofillAliases"
          :key-values="form.keyValues"
          :is-editing="isEditing"
          @select-kv="(kv) => (currentSelectedKv = kv)"
          @copy-kv="copyKvValue"
          @remove-kv="removeKeyValue"
          @add-kv="(focusEl) => addKeyValue(focusEl)"
          @update:current-kv-value="(v) => (currentKvValue = v)"
          @update:attachments="(v) => (attachments = v)"
          @update:attachments-to-add="(v) => (attachmentsToAdd = v)"
          @update:attachments-to-delete="(v) => (attachmentsToDelete = v)"
          @update:autofill-aliases="(v) => (form.autofillAliases = v)"
        />
      </template>

      <!-- History -->
      <template #history>
        <HaexSystemPasswordsEditorHistory
          :item-id="selectedItem?.id"
          class="h-full"
        />
      </template>
    </UTabs>

    <HaexSystemPasswordsDialogDeleteItem
      v-model:open="showDeleteDialog"
      :item-title="form.title"
      :final="isCurrentItemInTrash"
      @confirm="onDelete"
    />

    <HaexSystemPasswordsDialogDiscardChanges
      v-model:open="showDiscardDialog"
      :saving="saving"
      @confirm="onDiscardConfirmed"
      @save="onDiscardSave"
    />

    <HaexSystemPasswordsDrawerGenerator
      v-model:open="generatorOpen"
      v-model:value="form.password"
    />
  </form>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import {
  haexPasswordsItemDetails,
  haexPasswordsItemKeyValues,
  haexPasswordsItemBinaries,
} from '~/database/schemas'
import type { InsertHaexPasswordsItemDetails } from '~/database/schemas'
import { requireDb } from '~/stores/vault'
import { addBinaryAsync } from '~/utils/passwords/binaries'
import {
  createSnapshotAsync,
  loadCurrentAttachmentsAsSnapshotRefs,
} from '~/utils/passwords/snapshots'
import {
  usePasswordEditorForm,
  otpAlgorithms,
} from '~/composables/passwords/usePasswordEditorForm'

const { t } = useI18n()
const toast = useToast()

const passwordsStore = usePasswordsStore()
const tagsStore = usePasswordsTagsStore()
const groupsStore = usePasswordsGroupsStore()
const nav = usePasswordsNavigation()

const { getIconDescriptor } = useIconComponents()
const iconCacheStore = usePasswordsIconCacheStore()

const {
  selectedItem,
  isEditing,
  isCreating,
  form,
  formSnapshot,
  errors,
  attachments,
  attachmentsSnapshot,
  attachmentsToAdd,
  attachmentsToDelete,
  isDirty,
  currentSelectedKv,
  visibleKeyValues,
  currentKvValue,
  kvCopiedItem,
  copyKvValue,
  addKeyValue,
  removeKeyValue,
  isExpired,
  otpCode,
  otpFormatted,
  otpRemaining,
  otpDashArray,
  copiedOtp,
  copyOtp,
  revertForm,
} = usePasswordEditorForm()

const saving = ref(false)
const activeTab = ref('details')
const showDeleteDialog = ref(false)
const showDiscardDialog = ref(false)

const isCurrentItemInTrash = computed(() => {
  const itemId = selectedItem.value?.id
  if (!itemId) return false
  const groupId = groupsStore.itemGroupMap.get(itemId) ?? null
  if (!groupId) return false
  return groupsStore.isGroupInTrash(groupId)
})
const generatorOpen = ref(false)

const extraRef = ref<{ passkeysRef: { persistDeletionsAsync: () => Promise<void> } | null } | null>(null)

// ESC acts like the back button — triggers discard guard when dirty.
// Skip when a child modal is open; those handle ESC themselves.
const onKeydown = (event: KeyboardEvent) => {
  if (event.key !== 'Escape') return
  if (
    showDeleteDialog.value ||
    showDiscardDialog.value ||
    generatorOpen.value
  ) {
    return
  }
  event.preventDefault()
  onBack()
}
onMounted(() => {
  window.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
})

const tabItems = computed(() => [
  { label: t('tabs.details'), value: 'details', slot: 'details' as const },
  { label: t('tabs.extra'), value: 'extra', slot: 'extra' as const },
  { label: t('tabs.history'), value: 'history', slot: 'history' as const },
])

const iconDescriptor = computed(() => getIconDescriptor(form.icon || null))

const binaryIconSrc = computed(() => {
  if (iconDescriptor.value.kind !== 'binary') return null
  const src = iconCacheStore.getIconDataUrl(iconDescriptor.value.hash)
  if (src === null) {
    iconCacheStore.requestIcon(iconDescriptor.value.hash)
    return null
  }
  return src || null
})

const iconBackgroundStyle = computed(() =>
  form.color ? { backgroundColor: form.color } : undefined,
)

const loadKeyValuesAsync = async () => {
  if (!selectedItem.value?.id) return
  const db = requireDb()
  const rows = await db
    .select()
    .from(haexPasswordsItemKeyValues)
    .where(eq(haexPasswordsItemKeyValues.itemId, selectedItem.value.id))
  form.keyValues = rows.map((row) => ({
    id: row.id,
    key: row.key ?? '',
    value: row.value ?? '',
  }))
  formSnapshot.keyValues = JSON.parse(JSON.stringify(form.keyValues))
  currentSelectedKv.value = form.keyValues[0] ?? null
}

const loadAttachmentsAsync = async () => {
  if (!selectedItem.value?.id) return
  const db = requireDb()
  attachments.value = await db
    .select()
    .from(haexPasswordsItemBinaries)
    .where(eq(haexPasswordsItemBinaries.itemId, selectedItem.value.id))
  attachmentsSnapshot.value = JSON.parse(JSON.stringify(attachments.value))
}

onMounted(async () => {
  try {
    await tagsStore.loadTagsAsync()
  } catch (error) {
    console.error('[Editor] Failed to load tags:', error)
  }
  try {
    await loadKeyValuesAsync()
  } catch (error) {
    console.error('[Editor] Failed to load key-values:', error)
  }
  try {
    await loadAttachmentsAsync()
  } catch (error) {
    console.error('[Editor] Failed to load attachments:', error)
  }
})

const startEdit = () => {
  nav.startEdit()
}

const onBack = () => {
  if (isDirty.value) {
    showDiscardDialog.value = true
    return
  }
  // Existing-item edit → revert unsaved changes; create-cancel is a hard
  // drop to list, handled by the popped navigation state.
  if (isEditing.value && !isCreating.value) {
    revertForm()
  }
  nav.goBack()
}

const onDiscardConfirmed = () => {
  showDiscardDialog.value = false
  if (isEditing.value && !isCreating.value) {
    revertForm()
  }
  nav.goBack()
}

const onDelete = async () => {
  if (!selectedItem.value) return
  const id = selectedItem.value.id
  try {
    await passwordsStore.deleteItemAsync(id)
    showDeleteDialog.value = false
    passwordsStore.backToList()
    toast.add({ title: isCurrentItemInTrash.value ? t('toast.deleted') : t('toast.movedToTrash'), color: 'success' })
  } catch (error) {
    console.error('[Editor] Delete failed:', error)
    toast.add({
      title: t('toast.deleteError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const onRestore = async () => {
  if (!selectedItem.value) return
  try {
    await groupsStore.restoreItemAsync(selectedItem.value.id)
    await passwordsStore.loadItemsAsync()
    toast.add({ title: t('toast.restored'), color: 'success' })
  } catch (error) {
    console.error('[Editor] Restore failed:', error)
    toast.add({
      title: t('toast.restoreError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const onSave = async (): Promise<boolean> => {
  if (saving.value) return false

  errors.title = []
  errors.tags = []

  if (!form.title.trim()) {
    errors.title = [t('validation.titleRequired')]
    activeTab.value = 'details'
    return false
  }

  saving.value = true
  try {
    const db = requireDb()
    const itemId = selectedItem.value?.id ?? crypto.randomUUID()
    const now = new Date().toISOString()

    const payload: InsertHaexPasswordsItemDetails = {
      id: itemId,
      title: form.title.trim(),
      username: form.username.trim() || null,
      password: form.password || null,
      url: form.url.trim() || null,
      note: form.note || null,
      icon: form.icon.trim() || null,
      color: form.color || null,
      expiresAt: form.expiresAt || null,
      otpSecret: form.otpSecret.trim() || null,
      otpDigits: form.otpDigits || 6,
      otpPeriod: form.otpPeriod || 30,
      otpAlgorithm: form.otpAlgorithm,
      autofillAliases: Object.keys(form.autofillAliases).length
        ? form.autofillAliases
        : null,
      updatedAt: now,
    }

    if (isCreating.value) {
      await db
        .insert(haexPasswordsItemDetails)
        .values({ ...payload, createdAt: now })
      await groupsStore.setItemGroupAsync(itemId, groupsStore.selectedGroupId)
    } else {
      await db
        .update(haexPasswordsItemDetails)
        .set(payload)
        .where(eq(haexPasswordsItemDetails.id, itemId))
    }

    const resolvedTags = await tagsStore.resolveTagNamesAsync(form.tagNames)
    await tagsStore.setItemTagsAsync(
      itemId,
      resolvedTags.map((tag) => tag.id),
    )

    // Delete + re-insert key-values. Schema's $defaultFn assigns fresh IDs,
    // avoiding id-carryover across saves. The Rust CRDT layer wraps each
    // statement in its own transaction, so db.transaction() is unusable here.
    const keyValueRows = form.keyValues
      .filter((kv) => kv.key.trim())
      .map((kv) => ({
        itemId,
        key: kv.key.trim(),
        value: kv.value,
        updatedAt: now,
      }))
    await db
      .delete(haexPasswordsItemKeyValues)
      .where(eq(haexPasswordsItemKeyValues.itemId, itemId))
    if (keyValueRows.length > 0) {
      await db.insert(haexPasswordsItemKeyValues).values(keyValueRows)
    }

    // Persist renamed attachments
    for (const att of attachments.value) {
      const original = attachmentsSnapshot.value.find((entry) => entry.id === att.id)
      if (original && original.fileName !== att.fileName) {
        await db
          .update(haexPasswordsItemBinaries)
          .set({ fileName: att.fileName })
          .where(eq(haexPasswordsItemBinaries.id, att.id))
      }
    }

    // Process attachment deletions (junction row only — binary stays for dedup)
    for (const att of attachmentsToDelete.value) {
      await db
        .delete(haexPasswordsItemBinaries)
        .where(eq(haexPasswordsItemBinaries.id, att.id))
    }

    // Persist new attachments: upsert binary + insert junction row
    for (const att of attachmentsToAdd.value) {
      if (!att.data) continue
      const base64 = att.data.split(',')[1] ?? att.data
      const hash = await addBinaryAsync(base64, att.size ?? 0)
      await db.insert(haexPasswordsItemBinaries).values({
        itemId,
        binaryHash: hash,
        fileName: att.fileName,
      })
    }

    attachmentsToAdd.value = []
    attachmentsToDelete.value = []
    await loadAttachmentsAsync()
    await extraRef.value?.passkeysRef?.persistDeletionsAsync()

    // Snapshot captures the state just written — builds history timeline.
    const attachmentRefs = await loadCurrentAttachmentsAsSnapshotRefs(itemId)
    await createSnapshotAsync(
      itemId,
      {
        title: form.title.trim(),
        username: form.username.trim() || null,
        password: form.password || null,
        url: form.url.trim() || null,
        note: form.note || null,
        icon: form.icon.trim() || null,
        color: form.color || null,
        expiresAt: form.expiresAt || null,
        otpSecret: form.otpSecret.trim() || null,
        tagNames: form.tagNames,
        keyValues: form.keyValues.filter((kv) => kv.key.trim()),
        attachments: attachmentRefs,
      },
      now,
    )

    await passwordsStore.loadItemsAsync()
    passwordsStore.openItem(itemId)

    // Refresh the snapshot to the newly saved state.
    Object.assign(formSnapshot, JSON.parse(JSON.stringify(form)))

    toast.add({
      title: isCreating.value ? t('toast.created') : t('toast.updated'),
      color: 'success',
    })
    return true
  } catch (error) {
    console.error('[Editor] Save failed:', error)
    toast.add({
      title: t('toast.saveError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
    return false
  } finally {
    saving.value = false
  }
}

const onDiscardSave = async () => {
  const ok = await onSave()
  if (!ok) return
  showDiscardDialog.value = false
  nav.goBack()
}
</script>

<i18n lang="yaml">
de:
  tabs:
    details: Details
    extra: Extra
    history: Verlauf
  validation:
    titleRequired: Titel ist Pflicht
  toast:
    created: Eintrag erstellt
    updated: Eintrag aktualisiert
    deleted: Eintrag gelöscht
    movedToTrash: In Papierkorb verschoben
    restored: Wiederhergestellt
    saveError: Speichern fehlgeschlagen
    deleteError: Löschen fehlgeschlagen
    restoreError: Wiederherstellen fehlgeschlagen
en:
  tabs:
    details: Details
    extra: Extra
    history: History
  validation:
    titleRequired: Title is required
  toast:
    created: Entry created
    updated: Entry updated
    deleted: Entry deleted
    movedToTrash: Moved to trash
    restored: Restored
    saveError: Saving failed
    deleteError: Deletion failed
    restoreError: Restore failed
</i18n>
