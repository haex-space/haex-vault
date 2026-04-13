<template>
  <UiDrawerModal
    v-model:open="open"
    :title="editingClaim ? t('editTitle') : t('addTitle')"
  >
    <template #body>
      <div class="space-y-4">
        <USelectMenu
          v-if="!editingClaim"
          v-model="claimType"
          :items="claimTypeOptions"
          value-key="value"
          :label="t('type')"
          class="min-w-48"
        />
        <UiInput
          v-if="claimType === 'custom' && !editingClaim"
          v-model="customType"
          :label="t('customType')"
          placeholder="z.B. phone, company"
        />
        <UiInput
          v-model="value"
          :placeholder="placeholder"
          @keydown.enter.prevent="onSubmit"
        />
      </div>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UiButton
          icon="i-lucide-check"
          :disabled="!canSave"
          @click="onSubmit"
        >
          {{ t('save') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
export interface ClaimDialogEditTarget {
  id: string
  type: string
  value: string
}

export interface AddClaimPayload {
  mode: 'add'
  type: string
  value: string
}

export interface EditClaimPayload {
  mode: 'edit'
  claimId: string
  value: string
}

export type ClaimSubmitPayload = AddClaimPayload | EditClaimPayload

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  /** When set, the dialog is in "edit" mode for this existing claim. */
  editingClaim: ClaimDialogEditTarget | null
}>()

const emit = defineEmits<{
  submit: [payload: ClaimSubmitPayload]
}>()

const { t } = useI18n()

const claimType = ref('email')
const customType = ref('')
const value = ref('')

const claimTypeOptions = computed<
  { label: string; value: string; disabled?: boolean }[]
>(() => [
  { label: 'Email', value: 'email' },
  { label: 'Name', value: 'name' },
  { label: t('phone'), value: 'phone' },
  { label: t('address'), value: 'address' },
  { label: t('custom'), value: 'custom' },
])

const placeholder = computed(() => {
  if (props.editingClaim) return ''
  if (claimType.value === 'email') return 'user@example.com'
  if (claimType.value === 'name') return 'Max Mustermann'
  if (claimType.value === 'phone') return '+49 123 456789'
  if (claimType.value === 'address') return 'Musterstraße 1, 12345 Berlin'
  return ''
})

const canSave = computed(() => {
  if (!value.value.trim()) return false
  if (
    !props.editingClaim &&
    claimType.value === 'custom' &&
    !customType.value.trim()
  ) {
    return false
  }
  return true
})

// Reset form whenever the dialog opens — seed with editingClaim when present.
watch(
  () => [open.value, props.editingClaim] as const,
  ([isOpen, editing]) => {
    if (!isOpen) return
    if (editing) {
      value.value = editing.value
      claimType.value = editing.type
      customType.value = ''
    } else {
      const firstAvailable = claimTypeOptions.value.find((o) => !o.disabled)
      claimType.value = firstAvailable?.value ?? 'custom'
      customType.value = ''
      value.value = ''
    }
  },
)

const onSubmit = () => {
  if (!canSave.value) return
  if (props.editingClaim) {
    emit('submit', {
      mode: 'edit',
      claimId: props.editingClaim.id,
      value: value.value.trim(),
    })
    return
  }
  const type =
    claimType.value === 'custom' ? customType.value.trim() : claimType.value
  emit('submit', {
    mode: 'add',
    type,
    value: value.value.trim(),
  })
}
</script>

<i18n lang="yaml">
de:
  addTitle: Claim hinzufügen
  editTitle: Claim bearbeiten
  type: Typ
  customType: Eigener Typ
  phone: Telefon
  address: Adresse
  custom: Sonstiges
  save: Speichern
  cancel: Abbrechen
en:
  addTitle: Add Claim
  editTitle: Edit Claim
  type: Type
  customType: Custom Type
  phone: Phone
  address: Address
  custom: Custom
  save: Save
  cancel: Cancel
</i18n>
