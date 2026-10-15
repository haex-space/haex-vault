<script setup lang="ts">
/**
 * The question when contacts stop going to a phone (spec 051 FR-018d): delete them on that phone,
 * or keep them there without syncing them any more.
 */
import { toast } from 'vue-sonner'
import type { ContactsOnPhone } from '@bindings/ContactsOnPhone'

const props = defineProps<{
  ids: string[]
  device: string
  /** The phone's name. */
  phone: string
}>()

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  done: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const { targetsRemoveAsync } = useContactTargets()
const busy = ref<ContactsOnPhone | null>(null)

async function removeAsync(onPhone: ContactsOnPhone) {
  busy.value = onPhone
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // answered question of the window calls the command.
    await targetsRemoveAsync(props.ids, props.device, onPhone)
    await store.quietReloadAsync()
    open.value = false
    emit('done')
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    busy.value = null
  }
}
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => (open = value)"
  >
    <ShadcnAlertDialogContent data-testid="contacts-target-remove-dialog">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('contacts.targets.removeTitle', { phone })
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{
            t(
              'contacts.targets.removeQuestion',
              { phone, count: ids.length },
              ids.length,
            )
          }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('contacts.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="outline"
          :loading="busy === 'keep'"
          :disabled="busy !== null"
          data-testid="contacts-target-remove-keep"
          @click="removeAsync('keep')"
        >
          {{ t('contacts.targets.keepOnPhone') }}
        </UiButton>
        <UiButton
          variant="destructive"
          :loading="busy === 'delete'"
          :disabled="busy !== null"
          data-testid="contacts-target-remove-delete"
          @click="removeAsync('delete')"
        >
          {{ t('contacts.targets.deleteOnPhone') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
