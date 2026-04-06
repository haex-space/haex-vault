<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
  >
    <template #body>
      <UiInput
        v-model="form.label"
        :label="t('fields.label')"
      />
      <UiTextarea
        v-model="form.notes"
        :label="t('fields.notes')"
        :rows="2"
      />
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
          :loading="isEditing"
          :disabled="!form.label.trim()"
          @click="onSaveAsync"
        >
          {{ t('actions.save') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  contact: SelectHaexIdentities | null
}>()

const emit = defineEmits<{
  saved: []
}>()

const { t } = useI18n()
const { add: addToast } = useToast()
const identityStore = useIdentityStore()

const isEditing = ref(false)
const form = reactive({
  label: '',
  notes: '',
})

watch(() => props.contact, (contact) => {
  if (contact) {
    form.label = contact.label
    form.notes = contact.notes ?? ''
  }
}, { immediate: true })

const onSaveAsync = async () => {
  if (!form.label.trim() || !props.contact) return

  isEditing.value = true
  try {
    await identityStore.updateContactAsync(props.contact.id, {
      label: form.label.trim(),
      notes: form.notes.trim() || undefined,
    })
    addToast({ title: t('success.updated'), color: 'success' })
    open.value = false
    emit('saved')
  } catch (error) {
    console.error('Failed to update contact:', error)
    addToast({
      title: t('errors.updateFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isEditing.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  title: Kontakt bearbeiten
  fields:
    label: Name
    notes: Notizen
  actions:
    cancel: Abbrechen
    save: Speichern
  success:
    updated: Kontakt aktualisiert
  errors:
    updateFailed: Aktualisierung fehlgeschlagen
en:
  title: Edit Contact
  fields:
    label: Name
    notes: Notes
  actions:
    cancel: Cancel
    save: Save
  success:
    updated: Contact updated
  errors:
    updateFailed: Failed to update contact
</i18n>
