<script setup lang="ts">
/**
 * The view of one entry (spec 034, US1, FR-002, FR-003, FR-005, FR-006; spec 036, US1): the header,
 * the stale banner and the tabs Details and Extra with the fields, copy buttons, masked values, the
 * live TOTP block, attachments and passkeys. The detail the backend sends holds flags, never a
 * secret; the tab title stays the static place title, never an entry value (FR-040). The chosen
 * tab is part of the place (`?tab=`), so back, forward and a restored session find it again.
 */
import { DEFAULT_ENTRY_ICON } from '~/lib/passwords/icons'
import type { ItemDetail } from '@bindings/ItemDetail'
import type { BreadcrumbPlace } from '~/lib/passwords/breadcrumb'
import { displayTitle, isExpired, localDay } from '~/lib/passwords/format'
import { entryFreshness, type EntryFreshness } from '~/lib/passwords/remote'
import { isInTrash, trashGroupIds } from '~/lib/passwords/tree'
import { entryTab, withEntryTab, type EntryTab } from '~/lib/passwords/registry'

const props = defineProps<{
  itemId: string
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = usePasswordsStore()
const { getItemAsync, updateItemAsync } = usePasswords()

const TABS: readonly EntryTab[] = ['details', 'extra', 'history']
const activeTab = computed<EntryTab>({
  get: () => entryTab({ path: router.route.path, query: router.route.query }),
  set: (tab) =>
    router.replace(withEntryTab(props.itemId, tab, router.route.query)),
})

const detail = ref<ItemDetail | null>(null)
const deleteOpen = ref(false)
const notFound = ref(false)
const error = ref<string | null>(null)

async function loadAsync() {
  try {
    detail.value = await getItemAsync(props.itemId)
    notFound.value = false
    error.value = null
  } catch (cause) {
    if ((cause as { kind?: string }).kind === 'PasswordsNotFound') {
      detail.value = null
      notFound.value = true
    } else {
      error.value = errString(cause)
    }
  }
}

// A change from another window, an agent or another device does not replace what the user is
// looking at; a banner says so and offers to reload (US8). An own change reloads the entry itself,
// so the banner waits a moment before it shows, to let that settle.
const freshness = computed(() =>
  entryFreshness({
    overviewLoaded: store.hasLoadedOnce,
    present: store.headersById.has(props.itemId),
    loadedToken: detail.value?.updatedAt,
    currentToken: store.headersById.get(props.itemId)?.updatedAt,
  }),
)
const shownFreshness = ref<EntryFreshness>('fresh')
let freshnessTimer: ReturnType<typeof setTimeout> | null = null
watch(
  freshness,
  (value) => {
    if (freshnessTimer !== null) clearTimeout(freshnessTimer)
    if (value === 'fresh') {
      shownFreshness.value = 'fresh'
      return
    }
    freshnessTimer = setTimeout(() => {
      shownFreshness.value = freshness.value
    }, 400)
  },
  { immediate: true },
)
onBeforeUnmount(() => {
  if (freshnessTimer !== null) clearTimeout(freshnessTimer)
})

/** An entry deleted elsewhere leaves the history entry behind; skipping it keeps back from
 * returning to it. */
function leaveDeleted() {
  if (!router.skipCurrent()) router.replace('/')
}
onMounted(loadAsync)

const title = computed(() => displayTitle(detail.value?.title))
/** The folder the entry is in now: a move elsewhere shows at once, the loaded detail is older. */
const crumbPlace = computed<BreadcrumbPlace>(() => ({
  kind: 'entry',
  groupId:
    store.headersById.get(props.itemId)?.groupId ??
    detail.value?.groupId ??
    null,
}))
/** An entry in the trash can be looked at, not changed or deleted again. */
const inTrash = computed(() => {
  const groupId =
    store.headersById.get(props.itemId)?.groupId ?? detail.value?.groupId
  return isInTrash(groupId, trashGroupIds(store.groups))
})
const expired = computed(() =>
  isExpired(detail.value?.expiresAt, localDay(new Date())),
)

/** A restored state is the entry now: show it on Details. */
async function onRestored() {
  activeTab.value = 'details'
  await loadAsync()
}

function edit() {
  if (inTrash.value) return
  router.push(withEntryTab(props.itemId, activeTab.value, { edit: '' }))
}

/** Removes an invalid TOTP secret without touching anything else of the entry. */
async function removeOtpAsync() {
  if (!detail.value?.updatedAt) return
  try {
    await updateItemAsync(props.itemId, detail.value.updatedAt, {
      otpSecret: null,
    })
    await store.quietReloadAsync()
    await loadAsync()
  } catch (cause) {
    error.value = errString(cause)
  }
}
</script>

<template>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 @md:px-6">
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div class="flex items-center gap-2 pt-1">
        <PasswordsBreadcrumbs class="min-w-0 flex-1" :place="crumbPlace">
          <template v-if="detail" #default>
            <div class="flex min-w-0 items-baseline gap-2 px-1">
              <span
                class="size-7 shrink-0 self-center"
                :style="detail.color ? { color: detail.color } : undefined"
                aria-hidden="true"
              >
                <PasswordsEntryIcon
                  :value="detail.icon"
                  :fallback="DEFAULT_ENTRY_ICON"
                />
              </span>
              <h1
                class="truncate text-2xl font-bold text-foreground"
                :class="title === null ? 'text-muted-foreground italic' : ''"
                aria-current="page"
                data-testid="passwords-entry-title"
              >
                {{ title ?? t('passwords.untitled') }}
              </h1>
            </div>
          </template>
        </PasswordsBreadcrumbs>
        <ShadcnBadge
          v-if="detail?.owner"
          variant="secondary"
          class="shrink-0"
          data-testid="passwords-entry-owner"
        >
          {{
            t('passwords.owner', {
              name: t(`passwords.owners.${detail.owner}`),
            })
          }}
        </ShadcnBadge>
        <UiButton
          v-if="detail && !inTrash"
          variant="ghost"
          size="icon"
          class="ml-auto shrink-0"
          :aria-label="t('passwords.edit')"
          :tooltip="t('passwords.edit')"
          data-testid="passwords-edit"
          @click="edit"
        >
          <Icon name="lucide:pencil" class="size-4" />
        </UiButton>
        <UiButton
          v-if="detail && !inTrash"
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('passwords.delete.title')"
          :tooltip="t('passwords.delete.title')"
          data-testid="passwords-delete"
          @click="deleteOpen = true"
        >
          <Icon name="lucide:trash-2" class="size-4" />
        </UiButton>
      </div>

      <p
        v-if="notFound"
        class="py-10 text-center text-muted-foreground"
        role="alert"
      >
        {{ t('passwords.entryGone') }}
      </p>
      <p v-else-if="error" class="text-sm text-destructive" role="alert">
        {{ error }}
      </p>
      <div
        v-else-if="!detail"
        class="flex justify-center py-10 text-muted-foreground"
        role="status"
      >
        <Icon name="lucide:loader-circle" class="size-6 animate-spin" />
      </div>

      <template v-else>
        <div
          v-if="shownFreshness !== 'fresh'"
          class="flex flex-wrap items-center gap-2 rounded-xl bg-muted px-4 py-3 text-sm"
          role="status"
          data-testid="passwords-entry-stale"
        >
          <Icon name="lucide:refresh-cw" class="size-4 shrink-0" />
          <span class="min-w-0 flex-1">{{
            shownFreshness === 'deleted'
              ? t('passwords.stale.deleted')
              : t('passwords.stale.changed')
          }}</span>
          <UiButton
            v-if="shownFreshness === 'changed'"
            size="sm"
            data-testid="passwords-entry-reload"
            @click="loadAsync"
          >
            {{ t('passwords.stale.reload') }}
          </UiButton>
          <UiButton
            v-else
            size="sm"
            data-testid="passwords-entry-leave"
            @click="leaveDeleted"
          >
            {{ t('passwords.stale.back') }}
          </UiButton>
        </div>

        <div
          v-if="inTrash"
          class="flex items-center gap-2 rounded-xl bg-muted px-4 py-3 text-sm"
          role="status"
          data-testid="passwords-entry-in-trash"
        >
          <Icon name="lucide:trash-2" class="size-4 shrink-0" />
          <span class="min-w-0 flex-1">{{ t('passwords.inTrashNote') }}</span>
        </div>

        <ShadcnBadge v-if="expired" variant="destructive" class="self-start">
          {{ t('passwords.expiredOn', { date: detail.expiresAt }) }}
        </ShadcnBadge>

        <PasswordsEntryTabs v-model="activeTab" :tabs="TABS">
          <template #details>
            <PasswordsViewDetails
              :item-id="itemId"
              :detail="detail"
              @edit="edit"
              @remove-otp="removeOtpAsync"
            />
          </template>
          <template #extra>
            <PasswordsViewExtra
              :item-id="itemId"
              :detail="detail"
              @changed="loadAsync"
            />
          </template>
          <template #history>
            <PasswordsHistoryTab
              :item-id="itemId"
              :updated-at="detail.updatedAt"
              :active="activeTab === 'history'"
              @restored="onRestored"
            />
          </template>
        </PasswordsEntryTabs>
      </template>
    </div>

    <PasswordsDeleteDialog
      v-model:open="deleteOpen"
      :targets="[{ kind: 'item', id: itemId }]"
      @done="leaveDeleted"
    />
  </div>
</template>
