<script setup lang="ts">
/**
 * The confirmation before contacts are deleted (spec 051, FR-002): on every own device. It names the
 * file shares the contacts lose their access to (spec 056 FR-023).
 */
import { toast } from 'vue-sonner'
import type { FileShare } from '@bindings/FileShare'
import { useActionOrThrow } from '~/composables/useActionOrThrow'

const props = defineProps<{
  ids: string[]
  /** The name to show for one contact. */
  name?: string
}>()

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  done: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const { deleteAsync } = useContacts()
const busy = ref(false)
const sharesOf = useActionOrThrow('files.shares.ofContact')
/** The names of the file shares that lose the contacts. */
const affected = ref<string[]>([])
const lookupPending = ref(false)
const lookupError = ref<string | null>(null)

watch(open, async (isOpen, _previous, onCleanup) => {
  let stale = false
  onCleanup(() => (stale = true))
  affected.value = []
  lookupError.value = null
  lookupPending.value = false
  if (!isOpen) return
  lookupPending.value = true
  try {
    const lists = await Promise.all(
      props.ids.map(
        async (contactId) => (await sharesOf({ contactId })) as FileShare[],
      ),
    )
    if (stale) return
    affected.value = [...new Set(lists.flat().map((share) => share.name))]
  } catch (cause) {
    if (!stale) lookupError.value = errString(cause)
  } finally {
    if (!stale) lookupPending.value = false
  }
})

async function confirmAsync() {
  busy.value = true
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // confirmed delete of the window calls the command.
    await deleteAsync(props.ids)
    await store.quietReloadAsync()
    toast.success(t('contacts.delete.done', props.ids.length))
    open.value = false
    emit('done')
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => (open = value)"
  >
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('contacts.delete.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{
            ids.length === 1 && name
              ? t('contacts.delete.one', { name })
              : t('contacts.delete.many', { count: ids.length }, ids.length)
          }}
        </ShadcnAlertDialogDescription>
        <ShadcnAlertDialogDescription
          v-if="affected.length"
          data-testid="contacts-delete-file-shares"
        >
          {{
            t(
              'contacts.fileShares.affected',
              { names: affected.join(', ') },
              affected.length,
            )
          }}
        </ShadcnAlertDialogDescription>
        <ShadcnAlertDialogDescription
          v-if="lookupError"
          class="text-destructive"
          data-testid="contacts-delete-file-shares-error"
          role="alert"
        >
          {{ t('contacts.fileShares.lookupFailed') }}: {{ lookupError }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('contacts.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :loading="busy"
          :disabled="busy || lookupPending || lookupError !== null"
          data-testid="contacts-delete-confirm"
          @click="confirmAsync"
        >
          {{ t('contacts.delete.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
