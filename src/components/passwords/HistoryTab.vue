<script setup lang="ts">
/**
 * The tab Verlauf of an entry (spec 036, US2, FR-007 to FR-009; spec 034 US4, FR-017, FR-018): the
 * states in a dropdown above the chosen state; the newest is chosen first. "Restore" asks for
 * confirmation, makes the chosen state current and keeps the state it replaces. Restoring checks
 * that the entry has not changed meanwhile. Secrets of a state hide again when the state changes
 * and when the tab is left (`active` turns false).
 */
import { toast } from 'vue-sonner'
import type { SnapshotHeader } from '@bindings/SnapshotHeader'
import type { SnapshotView } from '@bindings/SnapshotView'

const props = defineProps<{
  itemId: string
  updatedAt: string | null
  /** Whether this is the tab on show. */
  active: boolean
  readonly?: boolean
}>()
const emit = defineEmits<{ restored: [] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = usePasswordsStore()
const { historyListAsync, historyGetAsync, historyRestoreAsync } =
  usePasswords()

const states = ref<SnapshotHeader[]>([])
const selected = ref<SnapshotView | null>(null)
const error = ref<string | null>(null)
const loading = ref(true)
const restoring = ref(false)
const confirming = ref(false)
const resetKey = ref(0)

watch(
  () => props.active,
  (on) => {
    if (!on) resetKey.value += 1
  },
)

async function selectAsync(snapshotId: string) {
  try {
    selected.value = await historyGetAsync(snapshotId)
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function loadAsync() {
  try {
    states.value = await historyListAsync(props.itemId)
    error.value = null
    const newest = states.value[0]
    if (newest) await selectAsync(newest.id)
  } catch (cause) {
    error.value = errString(cause)
  } finally {
    loading.value = false
  }
}

async function restoreAsync() {
  if (props.readonly) return
  const view = selected.value
  if (!view) return
  restoring.value = true
  try {
    // A change since the entry was loaded is a conflict, not an overwrite.
    if (!props.updatedAt) throw new Error('missing update token')
    const outcome = await historyRestoreAsync(
      props.itemId,
      view.id,
      props.updatedAt,
    )
    await store.quietReloadAsync()
    if (outcome.skippedAttachments.length > 0) {
      toast.warning(
        t('passwords.history.skipped', {
          files: outcome.skippedAttachments.join(', '),
        }),
      )
    } else {
      toast.success(t('passwords.history.restored'))
    }
    confirming.value = false
    emit('restored')
    await loadAsync()
  } catch (cause) {
    confirming.value = false
    toast.error(errString(cause))
  } finally {
    restoring.value = false
  }
}

onMounted(loadAsync)
</script>

<template>
  <div class="flex flex-col gap-4" data-testid="entry-history">
    <p v-if="error" class="text-sm text-destructive" role="alert">
      {{ error }}
    </p>
    <div
      v-else-if="loading"
      class="flex justify-center py-10 text-muted-foreground"
      role="status"
    >
      <Icon name="lucide:loader-circle" class="size-6 animate-spin" />
    </div>
    <template v-else>
      <p
        v-if="states.length <= 1"
        class="text-sm text-muted-foreground"
        data-testid="passwords-history-nothing-older"
      >
        {{ t('passwords.history.nothingOlder') }}
      </p>
      <div class="flex flex-col gap-4">
        <PasswordsHistoryTimeline
          :states="states"
          :selected-id="selected?.id ?? null"
          @select="selectAsync"
        />
        <PasswordsHistorySnapshot
          v-if="selected"
          :snapshot="selected"
          :restoring="restoring"
          :reset-key="resetKey"
          :readonly="readonly"
          @restore="confirming = true"
        />
      </div>
    </template>

    <ShadcnAlertDialog
      :open="confirming"
      @update:open="(value: boolean) => (confirming = value)"
    >
      <ShadcnAlertDialogContent>
        <ShadcnAlertDialogHeader>
          <ShadcnAlertDialogTitle>{{
            t('passwords.history.restoreTitle')
          }}</ShadcnAlertDialogTitle>
          <ShadcnAlertDialogDescription>
            {{ t('passwords.history.restoreBody') }}
          </ShadcnAlertDialogDescription>
        </ShadcnAlertDialogHeader>
        <ShadcnAlertDialogFooter>
          <ShadcnAlertDialogCancel>{{
            t('passwords.cancel')
          }}</ShadcnAlertDialogCancel>
          <UiButton
            :loading="restoring"
            data-testid="passwords-history-restore-confirm"
            @click="restoreAsync"
          >
            {{ t('passwords.history.restore') }}
          </UiButton>
        </ShadcnAlertDialogFooter>
      </ShadcnAlertDialogContent>
    </ShadcnAlertDialog>
  </div>
</template>
