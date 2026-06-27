<template>
  <div class="p-4 space-y-4">
    <!-- Card: Custom fields -->
    <HaexSystemPasswordsEditorKeyValues
      :visible-key-values="visibleKeyValues"
      :current-selected-kv="currentSelectedKv"
      :current-kv-value="currentKvValue"
      :kv-copied-item="kvCopiedItem"
      :is-editing="isEditing"
      @select-kv="(kv) => emit('selectKv', kv)"
      @copy-kv="(kv) => emit('copyKv', kv)"
      @remove-kv="(index) => emit('removeKv', index)"
      @add-kv="(focusEl) => emit('addKv', focusEl)"
      @update:current-kv-value="(v) => emit('update:currentKvValue', v)"
    />

    <!-- Card: Attachments -->
    <div class="border border-default rounded-lg overflow-hidden">
      <div class="px-4 py-3 border-b border-default bg-elevated/30">
        <div class="flex items-center gap-2">
          <UIcon
            name="i-lucide-paperclip"
            class="size-4 text-primary"
          />
          <p class="text-sm font-medium">
            {{ t('attachments.title') }}
          </p>
        </div>
        <p class="text-xs text-muted mt-0.5">
          {{ t('attachments.description') }}
        </p>
      </div>
      <div class="p-4">
        <HaexSystemPasswordsEditorAttachments
          v-model="attachmentsModel"
          v-model:attachments-to-add="attachmentsToAddModel"
          v-model:attachments-to-delete="attachmentsToDeleteModel"
          :read-only="!isEditing"
        />
      </div>
    </div>

    <!-- Card: Passkeys -->
    <div class="border border-default rounded-lg overflow-hidden">
      <div class="px-4 py-3 border-b border-default bg-elevated/30">
        <div class="flex items-center gap-2">
          <UIcon
            name="i-lucide-key-round"
            class="size-4 text-primary"
          />
          <p class="text-sm font-medium">
            {{ t('passkeys.title') }}
          </p>
        </div>
        <p class="text-xs text-muted mt-0.5">
          {{ t('passkeys.description') }}
        </p>
      </div>
      <div class="p-4">
        <HaexSystemPasswordsEditorPasskeys
          ref="passkeysRef"
          :item-id="itemId"
          :read-only="!isEditing"
        />
      </div>
    </div>

    <!-- Card: Autofill aliases -->
    <div class="border border-default rounded-lg overflow-hidden">
      <div class="px-4 py-3 border-b border-default bg-elevated/30">
        <div class="flex items-center gap-2">
          <UIcon
            name="i-lucide-globe"
            class="size-4 text-primary"
          />
          <p class="text-sm font-medium">
            {{ t('autofill.title') }}
          </p>
        </div>
        <p class="text-xs text-muted mt-0.5">
          {{ t('autofill.description') }}
        </p>
      </div>
      <div class="p-4">
        <HaexSystemPasswordsEditorAutofillAliases
          v-model="autofillAliasesModel"
          :key-values="keyValues"
          :read-only="!isEditing"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type {
  EditableKeyValue,
} from '~/composables/passwords/usePasswordEditorForm'
import type { AttachmentWithSize } from '~/types/passwords/attachment'

const { t } = useI18n()

defineProps<{
  // KeyValues
  visibleKeyValues: EditableKeyValue[]
  currentSelectedKv: EditableKeyValue | null
  currentKvValue: string
  kvCopiedItem: EditableKeyValue | null
  // Passkeys
  itemId: string | undefined
  // Autofill
  keyValues: EditableKeyValue[]
  // Shared
  isEditing: boolean
}>()

const emit = defineEmits<{
  // KeyValues
  selectKv: [kv: EditableKeyValue]
  copyKv: [kv: EditableKeyValue]
  removeKv: [index: number]
  addKv: [focusEl: { $el?: HTMLElement } | null]
  'update:currentKvValue': [value: string]
}>()

// Two-way bindings for attachments + autofill
const attachmentsModel = defineModel<AttachmentWithSize[]>('attachments', { required: true })
const attachmentsToAddModel = defineModel<AttachmentWithSize[]>('attachmentsToAdd', { required: true })
const attachmentsToDeleteModel = defineModel<AttachmentWithSize[]>('attachmentsToDelete', { required: true })
const autofillAliasesModel = defineModel<Record<string, string[]>>('autofillAliases', { required: true })

const passkeysRef = ref<{ persistDeletionsAsync: () => Promise<void> } | null>(null)

defineExpose({ passkeysRef })
</script>

<i18n lang="yaml">
de:
  passkeys:
    title: Passkeys
    description: Passkeys werden automatisch über die Browser-Erweiterung erstellt.
  attachments:
    title: Dateianhänge
    description: Dateien, Bilder und Dokumente die zu diesem Eintrag gehören.
  autofill:
    title: Autofill-Zuordnung
    description: Konfiguriere alternative Feldnamen für das Browser-Autofill.
en:
  passkeys:
    title: Passkeys
    description: Passkeys are created automatically via the browser extension.
  attachments:
    title: Attachments
    description: Files, images and documents associated with this entry.
  autofill:
    title: Autofill Mapping
    description: Configure alternative field names for browser autofill.
</i18n>
