<script setup lang="ts">
/**
 * The question after a run held back deletions (spec 051 FR-016): more than 10 contacts and more
 * than 10 % of the contacts synced with this phone would go at once. "Löschen" lets the run delete
 * them on the other side; "Behalten" keeps them (back into the address book, or onto the phone).
 */
import type { ContactsPhoneMassDecision } from '@bindings/ContactsPhoneMassDecision'
import type { ContactsPhoneMassDelete } from '@bindings/ContactsPhoneMassDelete'
import { toast } from 'vue-sonner'

defineProps<{ held: ContactsPhoneMassDelete | null }>()
const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ done: [] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const phone = useContactsPhone()
const busy = ref<ContactsPhoneMassDecision | null>(null)

async function decideAsync(decision: ContactsPhoneMassDecision) {
  busy.value = decision
  try {
    // action-exempt: deleting or keeping many contacts is the person's own decision (FR-016).
    await phone.massDeleteAsync(decision)
    await store.quietReloadAsync()
    toast.success(t(`contacts.massDelete.done.${decision}`))
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
    <ShadcnAlertDialogContent data-testid="contacts-mass-delete">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('contacts.massDelete.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t('contacts.massDelete.description') }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ul v-if="held" class="flex flex-col gap-1 text-sm">
        <li
          v-if="held.fromPhone > 0"
          data-testid="contacts-mass-delete-from-phone"
        >
          {{
            t(
              'contacts.massDelete.fromPhone',
              { count: held.fromPhone },
              held.fromPhone,
            )
          }}
        </li>
        <li
          v-if="held.fromHolzi > 0"
          data-testid="contacts-mass-delete-from-holzi"
        >
          {{
            t(
              'contacts.massDelete.fromHolzi',
              { count: held.fromHolzi },
              held.fromHolzi,
            )
          }}
        </li>
      </ul>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('contacts.massDelete.later')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="outline"
          :loading="busy === 'keep'"
          :disabled="busy !== null"
          data-testid="contacts-mass-delete-keep"
          @click="decideAsync('keep')"
        >
          {{ t('contacts.massDelete.keep') }}
        </UiButton>
        <UiButton
          variant="destructive"
          :loading="busy === 'delete'"
          :disabled="busy !== null"
          data-testid="contacts-mass-delete-delete"
          @click="decideAsync('delete')"
        >
          {{ t('contacts.massDelete.delete') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
