<template>
  <UiDrawerModal
    v-model:open="open"
    :title="editingClaim ? t('editTitle') : t('addTitle')"
  >
    <template #body>
      <div class="space-y-4">
        <USelect
          v-if="!editingClaim"
          v-model="claimType"
          class="min-w-48"
          :items="claimTypeOptions"
          value-key="value"
          :label="t('type')"
        />
        <UiInput
          v-if="claimType === 'custom' && !editingClaim"
          v-model="claimCustomType"
          :label="t('customType')"
          placeholder="z.B. phone, company"
        />
        <UiInput
          v-model="claimValue"
          :label="t('value')"
          :placeholder="claimValuePlaceholder"
          @keydown.enter.prevent="onSaveAsync"
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
          {{ t('actions.cancel') }}
        </UButton>
        <UiButton
          icon="i-lucide-check"
          :disabled="!canSave"
          @click="onSaveAsync"
        >
          {{ t('actions.save') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  contactId: string | null
  editingClaim: { id: string; contactId: string; type: string } | null
}>()

const emit = defineEmits<{
  saved: [contactId: string]
}>()

import { createLogger } from '@/stores/logging'

const log = createLogger('CONTACTS:CLAIMS')

const { t } = useI18n()
const { add: addToast } = useToast()
const identityStore = useIdentityStore()

const claimType = ref('email')
const claimCustomType = ref('')
const claimValue = ref('')

const claimTypeOptions = computed<{ label: string; value: string; disabled?: boolean }[]>(() => [
  { label: 'Email', value: 'email' },
  { label: 'Name', value: 'name' },
  { label: t('custom'), value: 'custom' },
])

const claimValuePlaceholder = computed(() => {
  if (props.editingClaim) return ''
  if (claimType.value === 'email') return 'user@example.com'
  if (claimType.value === 'name') return 'Max Mustermann'
  return ''
})

const canSave = computed(() => {
  if (!claimValue.value.trim()) return false
  if (!props.editingClaim && claimType.value === 'custom' && !claimCustomType.value.trim()) return false
  return true
})

watch(open, (isOpen) => {
  if (isOpen && !props.editingClaim) {
    const firstAvailable = claimTypeOptions.value.find(o => !o.disabled)
    claimType.value = firstAvailable?.value ?? 'custom'
    claimCustomType.value = ''
    claimValue.value = ''
  } else if (isOpen && props.editingClaim) {
    claimValue.value = ''
  }
})

const initEdit = (value: string) => {
  claimValue.value = value
}

const onSaveAsync = async () => {
  if (!canSave.value) return

  try {
    if (props.editingClaim) {
      log.info(`Updating claim ${props.editingClaim.id} (type: ${props.editingClaim.type})`)
      await identityStore.updateClaimAsync(
        props.editingClaim.id,
        claimValue.value.trim(),
      )
      log.info('Claim updated successfully')
      addToast({ title: t('updated'), color: 'success' })
      emit('saved', props.editingClaim.contactId)
    } else if (props.contactId) {
      const type = claimType.value === 'custom'
        ? claimCustomType.value.trim()
        : claimType.value
      log.info(`Adding claim: type="${type}" to contact ${props.contactId}`)
      await identityStore.addClaimAsync(
        props.contactId,
        type,
        claimValue.value.trim(),
      )
      log.info('Claim added successfully')
      addToast({ title: t('added'), color: 'success' })
      emit('saved', props.contactId)
    }
    open.value = false
  } catch (error) {
    log.error('Failed to save claim', error)
    addToast({
      title: t('saveFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

defineExpose({ initEdit })
</script>

<i18n lang="yaml">
de:
  addTitle: Claim hinzufügen
  editTitle: Claim bearbeiten
  type: Typ
  customType: Benutzerdefinierter Typ
  custom: Benutzerdefiniert
  value: Wert
  added: Claim hinzugefügt
  updated: Claim aktualisiert
  saveFailed: Claim konnte nicht gespeichert werden
  actions:
    cancel: Abbrechen
    save: Speichern
en:
  addTitle: Add Claim
  editTitle: Edit Claim
  type: Type
  customType: Custom Type
  custom: Custom
  value: Value
  added: Claim added
  updated: Claim updated
  saveFailed: Failed to save claim
  actions:
    cancel: Cancel
    save: Save
</i18n>
