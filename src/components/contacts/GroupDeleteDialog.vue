<script setup lang="ts">
/** The confirmation before a group is deleted (spec 051): its contacts stay. */
import { toast } from 'vue-sonner'
import type { ContactGroup } from '@bindings/ContactGroup'

const props = defineProps<{
  group: ContactGroup | null
}>()

const emit = defineEmits<{
  close: []
  deleted: [id: string]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const { groupDeleteAsync } = useContacts()
const busy = ref(false)

async function confirmAsync() {
  const group = props.group
  if (!group) return
  busy.value = true
  try {
    await groupDeleteAsync(group.id)
    await store.quietReloadAsync()
    emit('deleted', group.id)
    emit('close')
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <ShadcnAlertDialog
    :open="group !== null"
    @update:open="(value: boolean) => !value && emit('close')"
  >
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('contacts.groups.deleteTitle')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t('contacts.groups.deleteText', { name: group?.name ?? '' }) }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('contacts.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="contacts-group-delete-confirm"
          @click="confirmAsync"
        >
          {{ t('contacts.groups.delete') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
