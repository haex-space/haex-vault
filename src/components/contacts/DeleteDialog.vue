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

watch(open, async (isOpen) => {
  affected.value = []
  if (!isOpen) return
  const lists = await Promise.all(
    props.ids.map((contactId) =>
      sharesOf({ contactId }).then(
        (shares) => shares as FileShare[],
        () => [],
      ),
    ),
  )
  affected.value = [...new Set(lists.flat().map((share) => share.name))]
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
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('contacts.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="contacts-delete-confirm"
          @click="confirmAsync"
        >
          {{ t('contacts.delete.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
