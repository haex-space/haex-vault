<script setup lang="ts">
/**
 * The editor of an entry (spec 034, US1, FR-001..FR-003, FR-005, research R7 and R15; spec 036,
 * US1): create at `/entry/new`, edit at `/entry/:id?edit`, in the tabs Details and Extra (the
 * fields live in `EditorDetails.vue` and `EditorExtra.vue`; Verlauf is for the saved entry and is
 * not offered here). The title is optional. A save that fails at a field jumps to the tab holding
 * it. The password and the custom values load as stored (placeholders unresolved) and are sent
 * only when they changed. A save that finds the entry changed or
 * deleted meanwhile asks what to do instead of overwriting (`ConflictDialog`), and leaving with
 * unsaved changes asks too (`UnsavedDialog`). The draft is a local object that the quiet reload of
 * the store never touches.
 */
import { toast } from 'vue-sonner'
import type { ItemDetail } from '@bindings/ItemDetail'
import {
  cloneDraft,
  draftFromDetail,
  emptyDraft,
  isDirty,
  toInput,
  toPatch,
  type Draft,
} from '~/lib/passwords/draft'
import { displayTitle } from '~/lib/passwords/format'
import { entryTab, withEntryTab, type EntryTab } from '~/lib/passwords/registry'

const props = defineProps<{
  /** `null` creates a new entry. */
  itemId: string | null
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const tab = useWmTab()
const store = usePasswordsStore()
const { getItemAsync, createItemAsync, updateItemAsync, revealAsync } =
  usePasswords()

const TABS: readonly EntryTab[] = ['details', 'extra']
const activeTab = computed<EntryTab>({
  get: () => {
    const shown = entryTab({
      path: router.route.path,
      query: router.route.query,
    })
    return shown === 'extra' ? 'extra' : 'details'
  },
  set: (next) =>
    router.replace(
      withEntryTab(props.itemId ?? 'new', next, router.route.query),
    ),
})

const detail = ref<ItemDetail | null>(null)
const initial = ref<Draft>(emptyDraft())
const draft = ref<Draft>(emptyDraft())
const loading = ref(props.itemId !== null)
/** Whether the entry could not be loaded; the message went out as a toast. */
const loadFailed = ref(false)
const saving = ref(false)
const otpError = ref<string | null>(null)
const saveError = ref<string | null>(null)
const conflict = ref<'changed' | 'deleted' | null>(null)
const askingToLeave = ref(false)

const isNew = computed(() => props.itemId === null)
const dirty = computed(() => isDirty(initial.value, draft.value))

/** Refreshes only the attachment list: the draft and the update token stay as they are. */
async function reloadAttachmentsAsync() {
  if (props.itemId === null || !detail.value) return
  try {
    const fresh = await getItemAsync(props.itemId)
    detail.value = { ...detail.value, attachments: fresh.attachments }
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function loadAsync() {
  if (props.itemId === null) return
  loading.value = true
  try {
    const [loaded, password] = await Promise.all([
      getItemAsync(props.itemId),
      revealAsync(props.itemId, { kind: 'storedPassword' }),
    ])
    detail.value = loaded
    initial.value = draftFromDetail(loaded, password.value)
    draft.value = cloneDraft(initial.value)
    loadFailed.value = false
  } catch (cause) {
    loadFailed.value = true
    toast.error(errString(cause))
  } finally {
    loading.value = false
  }
}

onMounted(loadAsync)

// Leaving the tab or window with unsaved changes is guarded by the window manager.
const unregister = tab.registerCloseGuard(() =>
  dirty.value
    ? {
        reasonKey: 'passwords.unsaved.reason',
        confirmAsync: async () => {},
      }
    : null,
)
onBeforeUnmount(() => unregister())

function viewPath(id: string): string {
  return `/entry/${id}`
}

/** The saved entry on the tab the user was on. */
function viewPlace(id: string) {
  return withEntryTab(id, activeTab.value, {})
}

function leave() {
  if (props.itemId !== null) router.replace(viewPlace(props.itemId))
  else if (router.canGoBack) router.back()
  else router.replace('/')
}

function requestLeave() {
  if (dirty.value) askingToLeave.value = true
  else leave()
}

const OTP_FIELD_ERRORS = new Set([
  'otpSecret',
  'otpDigits',
  'otpPeriod',
  'otpAlgorithm',
])

async function saveAsync(): Promise<boolean> {
  saving.value = true
  otpError.value = null
  saveError.value = null
  try {
    if (props.itemId === null) {
      const created = await createItemAsync(toInput(draft.value))
      await store.quietReloadAsync()
      router.replace(viewPath(created.itemId))
      return true
    }
    const patch = toPatch(initial.value, draft.value)
    if (Object.keys(patch).length > 0) {
      const updatedAt = detail.value?.updatedAt
      if (!updatedAt) throw new Error('missing update token')
      await updateItemAsync(props.itemId, updatedAt, patch)
      await store.quietReloadAsync()
    }
    // Reset the baseline so the guard does not fire on the way out.
    initial.value = cloneDraft(draft.value)
    router.replace(viewPlace(props.itemId))
    return true
  } catch (cause) {
    const error = cause as { kind?: string; reason?: string }
    if (
      error.kind === 'InvalidInput' &&
      OTP_FIELD_ERRORS.has(error.reason ?? '')
    ) {
      otpError.value = t(`passwords.editor.otpErrors.${error.reason}`)
      // The TOTP inputs are on the tab Details.
      activeTab.value = 'details'
    } else if (error.kind === 'PasswordsConflict') {
      conflict.value = error.reason === 'deleted' ? 'deleted' : 'changed'
    } else if (error.kind === 'PasswordsReferenceCycle') {
      // Spec 036, FR-046: name the entry the loop runs through.
      const source = store.headersById.get(
        (cause as { sourceItemId?: string }).sourceItemId ?? '',
      )
      saveError.value = t('errors.passwords.referenceCycleNamed', {
        title: displayTitle(source?.title) ?? t('passwords.untitled'),
      })
    } else {
      saveError.value = errString(cause)
    }
    return false
  } finally {
    saving.value = false
  }
}

async function saveAndLeaveAsync() {
  askingToLeave.value = false
  await saveAsync()
}

function discardAndLeave() {
  askingToLeave.value = false
  draft.value = cloneDraft(initial.value)
  leave()
}

/** The draft with the stored TOTP secret if the user did not touch it (every other value is in the
 * draft already), so it can become a new entry. A deleted entry cannot be read any more; the
 * secret is then not part of it. */
async function materialiseAsync(): Promise<Draft> {
  const filled = cloneDraft(draft.value)
  const current = detail.value
  if (props.itemId === null || !current || conflict.value === 'deleted')
    return filled
  if (filled.otp.mode === 'keep' && current.hasOtpSecret) {
    const revealed = await revealAsync(props.itemId, { kind: 'otpSecret' })
    filled.otp = {
      mode: 'set',
      text: revealed.value,
      digits: current.otpDigits,
      period: current.otpPeriod,
      algorithm: current.otpAlgorithm,
    }
  }
  return filled
}

async function saveAsNewAsync() {
  saving.value = true
  try {
    const created = await createItemAsync(toInput(await materialiseAsync()))
    conflict.value = null
    initial.value = cloneDraft(draft.value)
    await store.quietReloadAsync()
    router.replace(viewPath(created.itemId))
  } catch (cause) {
    conflict.value = null
    saveError.value = errString(cause)
  } finally {
    saving.value = false
  }
}

async function takeCurrentAsync() {
  conflict.value = null
  await loadAsync()
  if (props.itemId) router.replace(viewPlace(props.itemId))
}

watch(saveError, (message) => {
  if (message) toast.error(message)
})
</script>

<template>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 @md:px-6">
    <form
      class="mx-auto flex w-full max-w-3xl flex-col gap-5"
      data-testid="passwords-editor"
      @submit.prevent="saveAsync"
    >
      <div class="flex items-center gap-2 pt-1">
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          class="-ml-2 shrink-0"
          :aria-label="t('passwords.cancel')"
          :tooltip="t('passwords.cancel')"
          data-testid="passwords-editor-cancel"
          @click="requestLeave"
        >
          <Icon name="lucide:arrow-left" class="size-5" />
        </UiButton>
        <h1 class="min-w-0 flex-1 truncate text-2xl font-bold">
          {{
            isNew
              ? t('passwords.editor.newTitle')
              : t('passwords.editor.editTitle')
          }}
        </h1>
        <UiButton
          type="submit"
          class="shrink-0"
          :loading="saving"
          :disabled="loading || (!isNew && !dirty)"
          data-testid="passwords-editor-save"
        >
          {{ t('passwords.save') }}
        </UiButton>
      </div>

      <div
        v-if="loading"
        class="flex justify-center py-10 text-muted-foreground"
        role="status"
      >
        <Icon name="lucide:loader-circle" class="size-6 animate-spin" />
      </div>

      <template v-else-if="!loadFailed">
        <PasswordsEntryTabs v-model="activeTab" :tabs="TABS">
          <template #details>
            <PasswordsEditorDetails
              v-model="draft"
              :item-id="itemId"
              :detail="detail"
              :otp-error="otpError"
            />
          </template>
          <template #extra>
            <PasswordsEditorExtra
              v-model="draft"
              :item-id="itemId"
              :detail="detail"
              @changed="reloadAttachmentsAsync"
            />
          </template>
        </PasswordsEntryTabs>
      </template>
    </form>

    <PasswordsConflictDialog
      :open="conflict !== null"
      :reason="conflict ?? 'changed'"
      @save-as-new="saveAsNewAsync"
      @take-current="takeCurrentAsync"
      @close="conflict = null"
    />
    <PasswordsUnsavedDialog
      :open="askingToLeave"
      @save="saveAndLeaveAsync"
      @discard="discardAndLeave"
      @stay="askingToLeave = false"
    />
  </div>
</template>
