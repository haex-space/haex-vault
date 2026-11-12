<script setup lang="ts">
/**
 * The editor of a pass (spec 092, FR-002, FR-003, FR-012): create at `/entry/new?type=pass`, edit at
 * `/entry/:id?edit`, in the tabs Details and Extra (the free fields and the attachments). It works
 * like `EntryEditor.vue` for a login: the draft is a local object that the quiet reload of the store
 * never touches; a save that finds the pass changed or deleted meanwhile asks what to do instead of
 * overwriting; leaving with unsaved changes asks too. A code that does not fit its format is
 * refused with its reason, shown at the code. The codes load only for this pass, when the editor
 * opens.
 */
import { toast } from 'vue-sonner'
import type { ItemDetail } from '@bindings/ItemDetail'
import {
  clonePassDraft,
  draftIndexOfPosition,
  emptyPassDraft,
  isPassDirty,
  passDraftFromDetail,
  startIsValid,
  toPassInput,
  toPassPatch,
  type PassDraft,
} from '~/lib/passwords/passDraft'
import { entryTab, withEntryTab, type EntryTab } from '~/lib/passwords/registry'

const props = defineProps<{
  /** `null` creates a new pass. */
  itemId: string | null
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const tab = useWmTab()
const store = usePasswordsStore()
const { getItemAsync } = usePasswords()
const { createPassAsync, updatePassAsync, codesAsync } = usePasses()

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

/** A new pass starts with the kind and the link the place names (`?parent=`, spec 092 FR-014). */
function startingDraft(): PassDraft {
  const draft = emptyPassDraft()
  const parent = router.route.query.parent
  if (typeof parent === 'string' && parent !== '') draft.parentId = parent
  return draft
}

const detail = ref<ItemDetail | null>(null)
const initial = ref<PassDraft>(startingDraft())
const draft = ref<PassDraft>(clonePassDraft(initial.value))
const loading = ref(props.itemId !== null)
const loadFailed = ref(false)
const saving = ref(false)
const saveError = ref<string | null>(null)
const refusedCodes = ref<Record<number, string>>({})
const conflict = ref<'changed' | 'deleted' | null>(null)
const askingToLeave = ref(false)

const isNew = computed(() => props.itemId === null)
const dirty = computed(() => isPassDirty(initial.value, draft.value))
/** A start that was changed and cannot be sent; an unchanged one is never sent, so it never blocks. */
const startInvalid = computed(
  () =>
    JSON.stringify(draft.value.start) !== JSON.stringify(initial.value.start) &&
    !startIsValid(draft.value.start),
)

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
    const [loaded, codes] = await Promise.all([
      getItemAsync(props.itemId),
      codesAsync(props.itemId),
    ])
    detail.value = loaded
    initial.value = passDraftFromDetail(loaded, codes)
    draft.value = clonePassDraft(initial.value)
    loadFailed.value = false
  } catch (cause) {
    loadFailed.value = true
    toast.error(errString(cause))
  } finally {
    loading.value = false
  }
}

onMounted(loadAsync)

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

/** Shows what the backend refused at the field, and returns whether it did. */
function showRefusal(cause: unknown): boolean {
  const error = cause as {
    kind?: string
    code?: string
    position?: number | null
    reason?: string | null
  }
  if (error.kind === 'PassRefused' && error.code === 'code_invalid') {
    const index = draftIndexOfPosition(draft.value.codes, error.position ?? -1)
    if (index !== null) {
      refusedCodes.value = { [index]: error.reason ?? 'cannot_encode' }
      activeTab.value = 'details'
      return true
    }
  }
  if (
    error.kind === 'InvalidInput' &&
    (error.reason === 'start' || error.reason === 'startZone')
  ) {
    activeTab.value = 'details'
    saveError.value = t(
      error.reason === 'start'
        ? 'passwords.pass.errors.start'
        : 'passwords.pass.errors.zone',
    )
    return true
  }
  return false
}

async function saveAsync(): Promise<boolean> {
  if (startInvalid.value) {
    activeTab.value = 'details'
    return false
  }
  saving.value = true
  saveError.value = null
  refusedCodes.value = {}
  try {
    if (props.itemId === null) {
      const created = await createPassAsync(toPassInput(draft.value))
      await store.quietReloadAsync()
      router.replace(viewPath(created.id))
      return true
    }
    const patch = toPassPatch(initial.value, draft.value)
    if (Object.keys(patch).length > 0) {
      const updatedAt = detail.value?.updatedAt
      if (!updatedAt) throw new Error('missing update token')
      await updatePassAsync(props.itemId, updatedAt, patch)
      await store.quietReloadAsync()
    }
    initial.value = clonePassDraft(draft.value)
    router.replace(viewPlace(props.itemId))
    return true
  } catch (cause) {
    const error = cause as { kind?: string; reason?: string }
    if (error.kind === 'PasswordsConflict') {
      conflict.value = error.reason === 'deleted' ? 'deleted' : 'changed'
    } else if (!showRefusal(cause)) {
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
  draft.value = clonePassDraft(initial.value)
  leave()
}

/** The draft as a new pass, for a conflict. The codes go with their ids: the backend gives every
 * copy a row of its own and lets a code that arrived flagged keep its flag. */
async function saveAsNewAsync() {
  saving.value = true
  try {
    const created = await createPassAsync(
      toPassInput(draft.value, { keepCodeIds: true }),
    )
    conflict.value = null
    initial.value = clonePassDraft(draft.value)
    await store.quietReloadAsync()
    router.replace(viewPath(created.id))
  } catch (cause) {
    conflict.value = null
    if (!showRefusal(cause)) saveError.value = errString(cause)
  } finally {
    saving.value = false
  }
}

async function takeCurrentAsync() {
  conflict.value = null
  await loadAsync()
  if (props.itemId) router.replace(viewPlace(props.itemId))
}

// What the backend refused at a code no longer holds once the codes change: the reason would
// otherwise stay on a corrected value, or move to another row.
watch(
  () => draft.value.codes,
  () => {
    refusedCodes.value = {}
  },
  { deep: true },
)

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
            isNew ? t('passwords.pass.newTitle') : t('passwords.pass.editTitle')
          }}
        </h1>
        <UiButton
          type="submit"
          class="shrink-0"
          :loading="saving"
          :disabled="loading || (!isNew && !dirty) || startInvalid"
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
            <PasswordsPassEditorDetails
              v-model="draft"
              :refused-codes="refusedCodes"
              :start-invalid="startInvalid"
            />
          </template>
          <template #extra>
            <div class="flex flex-col gap-5" data-testid="pass-editor-extra">
              <SettingsGroup :label="t('passwords.fields.custom')">
                <li class="px-4 py-3" data-no-swipe>
                  <PasswordsKeyValues
                    v-model="draft.keyValues"
                    :item-id="itemId"
                  />
                </li>
              </SettingsGroup>
              <PasswordsAttachments
                v-if="itemId !== null && detail"
                :item-id="itemId"
                :attachments="detail.attachments"
                @changed="reloadAttachmentsAsync"
              />
            </div>
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
