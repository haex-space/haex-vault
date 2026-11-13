<script setup lang="ts">
/**
 * The confirmation before the trash is emptied (spec 034, US4, FR-015): it names the number of
 * entries and folders that go for good, and that this cannot be undone.
 */
import { toast } from 'vue-sonner'
import type { Target } from '@bindings/Target'

const props = defineProps<{
  entries: number
  folders: number
  /** The entries in the trash, for the references other entries hold on them (spec 036). */
  itemIds: readonly string[]
  /** What goes: the folders and entries deleted directly in this trash's scope (spec 092, Phase W:
   * the trash of the password manager and the one of Wallet are two views of one trash and may not
   * empty each other's). Without it the whole trash is emptied. */
  targets?: readonly Target[]
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { errString } = useErrorString()
const store = usePasswordsStore()
const { emptyTrashAsync, deletePermanentlyAsync } = usePasswords()
const busy = ref(false)
const inlineReferences = ref(true)

watch(open, (isOpen) => {
  if (isOpen) inlineReferences.value = true
})

async function confirmAsync() {
  busy.value = true
  try {
    if (props.targets) {
      await deletePermanentlyAsync([...props.targets], inlineReferences.value)
    } else {
      await emptyTrashAsync(inlineReferences.value)
    }
    await store.quietReloadAsync()
    open.value = false
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
          t('passwords.trash.emptyTitle')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription data-testid="passwords-empty-trash-count">
          {{ t('passwords.trash.emptyBody', { entries, folders }) }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <PasswordsReferenceUsageNote
        v-if="open"
        v-model:inline="inlineReferences"
        :item-ids="itemIds"
      />
      <PasswordsPasskeyDeleteNote :item-ids="itemIds" />
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('passwords.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="passwords-empty-trash-confirm"
          @click="confirmAsync"
        >
          {{ t('passwords.trash.empty') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
