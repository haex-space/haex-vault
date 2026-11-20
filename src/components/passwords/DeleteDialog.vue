<script setup lang="ts">
/**
 * The confirmation before something goes into the trash (spec 034, US4, FR-015, FR-034): it names
 * what is deleted, says that it goes to the trash and can be restored, says that a folder takes its
 * content with it, and warns when a haex-vault function uses one of the entries (the delete stays
 * allowed). Used for one entry, for a selection and for a folder.
 */
import { toast } from 'vue-sonner'
import type { Target } from '@bindings/Target'
import {
  usageWarning,
  type ItemUsage,
  type UsageWarning,
} from '~/lib/passwords/usage'

const props = defineProps<{
  /** What is to be deleted. */
  targets: Target[]
}>()

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  /** The delete ran; the caller leaves the place of the deleted thing. */
  done: []
}>()

const { t, te } = useI18n()

/** The name of a haex-vault function that uses an entry, as the owner badge shows it (spec 038). */
const featureName = (feature: string) =>
  te(`passwords.owners.${feature}`) ? t(`passwords.owners.${feature}`) : feature
const { errString } = useErrorString()
const store = usePasswordsStore()
const { trashAsync, itemUsageAsync } = usePasswords()

const warning = ref<UsageWarning | null>(null)
const busy = ref(false)

const itemCount = computed(
  () => props.targets.filter((target) => target.kind === 'item').length,
)
const folderCount = computed(
  () => props.targets.filter((target) => target.kind === 'group').length,
)

watch(open, async (isOpen) => {
  warning.value = null
  if (!isOpen) return
  const ids = props.targets
    .filter((target) => target.kind === 'item')
    .map((target) => target.id)
  try {
    const results: ItemUsage[] = await Promise.all(
      ids.map(async (itemId) => ({
        itemId,
        features: (await itemUsageAsync(itemId)).features,
      })),
    )
    warning.value = usageWarning(results)
  } catch {
    // A failed check must not stop the user from deleting; there is just no warning.
    warning.value = null
  }
})

async function confirmAsync() {
  busy.value = true
  try {
    await trashAsync(props.targets)
    await store.quietReloadAsync()
    toast.success(t('passwords.delete.done'))
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
          t('passwords.delete.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription as-child>
          <div class="flex flex-col gap-2">
            <p data-testid="passwords-delete-count">
              <template v-if="itemCount > 0">{{
                t('passwords.delete.items', { count: itemCount }, itemCount)
              }}</template>
              <template v-if="itemCount > 0 && folderCount > 0"> · </template>
              <template v-if="folderCount > 0">{{
                t(
                  'passwords.delete.folders',
                  { count: folderCount },
                  folderCount,
                )
              }}</template>
            </p>
            <p>{{ t('passwords.delete.toTrash') }}</p>
            <p v-if="folderCount > 0">
              {{ t('passwords.delete.folderContent') }}
            </p>
            <p
              v-if="warning"
              class="rounded-lg border border-destructive/40 p-2 text-destructive"
              role="alert"
              data-testid="passwords-delete-usage"
            >
              {{
                t('passwords.delete.usedBy', {
                  features: warning.features.map(featureName).join(', '),
                })
              }}
            </p>
          </div>
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('passwords.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :loading="busy"
          data-testid="passwords-delete-confirm"
          @click="confirmAsync"
        >
          {{ t('passwords.delete.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
