<script setup lang="ts">
/**
 * The view of one pass (spec 092-wallet-passes, Phase W): the header with back, edit and delete, the
 * stale banner, the "Belongs to" line, and the tabs Details (the codes, the dates, what links to it)
 * and Extra (custom fields and attachments, no autofill aliases and no passkeys) and Verlauf. The
 * detail the backend sends holds flags, never a code; the tab title stays the static place title.
 * The chosen tab is part of the place (`?tab=`), so back, forward and a restored session find it
 * again.
 */
import { DEFAULT_ENTRY_ICON } from '~/lib/passwords/icons'
import { passFallbackIcon } from '~/lib/passwords/passType'
import { entryTab, withEntryTab, type EntryTab } from '~/lib/passwords/registry'
import { walletNewPassLocation } from '~/lib/wallet/registry'

const props = defineProps<{
  itemId: string
}>()

const { t } = useI18n()
const router = useTabRouter()

const TABS: readonly EntryTab[] = ['details', 'extra', 'history']
const activeTab = computed<EntryTab>({
  get: () => entryTab({ path: router.route.path, query: router.route.query }),
  set: (tab) =>
    router.replace(withEntryTab(props.itemId, tab, router.route.query)),
})

const deleteOpen = ref(false)
const moveOpen = ref(false)
const store = usePasswordsStore()
const {
  detail,
  notFound,
  loadFailed,
  loadAsync,
  shownFreshness,
  leaveDeleted,
  title,
  inTrash,
  expired,
} = useEntryDetail(() => props.itemId)

/** A restored state is the pass now: show it on Details. */
async function onRestored() {
  activeTab.value = 'details'
  await loadAsync()
}

/** A new pass that belongs to this one (spec 092, FR-014). */
function addLinked() {
  void router.push(walletNewPassLocation(props.itemId))
}

/** A link changed (cleared): the pass reads again, with the headers of the store. */
async function onLinkChanged() {
  await store.quietReloadAsync()
  await loadAsync()
}

function edit() {
  if (inTrash.value) return
  router.push(withEntryTab(props.itemId, activeTab.value, { edit: '' }))
}

function back() {
  if (router.canGoBack) router.back()
  else router.replace('/')
}
</script>

<template>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 @md:px-6">
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div class="flex items-center gap-2 pt-1">
        <UiButton
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('wallet.back')"
          :tooltip="t('wallet.back')"
          data-testid="wallet-back"
          @click="back"
        >
          <Icon name="lucide:arrow-left" class="size-4" />
        </UiButton>
        <div
          v-if="detail"
          class="flex min-w-0 flex-1 items-baseline gap-2 px-1"
        >
          <span
            class="size-7 shrink-0 self-center"
            :style="detail.color ? { color: detail.color } : undefined"
            aria-hidden="true"
          >
            <PasswordsEntryIcon
              :value="detail.icon"
              :fallback="passFallbackIcon(detail) ?? DEFAULT_ENTRY_ICON"
            />
          </span>
          <h1
            class="truncate text-2xl font-bold text-foreground"
            :class="title === null ? 'text-muted-foreground italic' : ''"
            aria-current="page"
            data-testid="wallet-title"
          >
            {{ title ?? t('passwords.untitled') }}
          </h1>
        </div>
        <div v-else class="flex-1" />
        <UiButton
          v-if="detail && !inTrash"
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('passwords.pass.links.add')"
          :tooltip="t('passwords.pass.links.add')"
          data-testid="wallet-add-linked"
          @click="addLinked"
        >
          <Icon name="lucide:link" class="size-4" />
        </UiButton>
        <UiButton
          v-if="detail && !inTrash"
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('wallet.move.open')"
          :tooltip="t('wallet.move.open')"
          data-testid="wallet-move"
          @click="moveOpen = true"
        >
          <Icon name="lucide:folder-input" class="size-4" />
        </UiButton>
        <UiButton
          v-if="detail && !inTrash"
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('passwords.edit')"
          :tooltip="t('passwords.edit')"
          data-testid="wallet-edit"
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
          data-testid="wallet-delete"
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
        {{ t('wallet.gone') }}
      </p>
      <div
        v-else-if="!detail && !loadFailed"
        class="flex justify-center py-10 text-muted-foreground"
        role="status"
      >
        <Icon name="lucide:loader-circle" class="size-6 animate-spin" />
      </div>

      <template v-else-if="detail">
        <div
          v-if="shownFreshness !== 'fresh'"
          class="flex flex-wrap items-center gap-2 rounded-xl bg-muted px-4 py-3 text-sm"
          role="status"
          data-testid="wallet-stale"
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
            data-testid="wallet-reload"
            @click="loadAsync"
          >
            {{ t('passwords.stale.reload') }}
          </UiButton>
          <UiButton
            v-else
            size="sm"
            data-testid="wallet-leave"
            @click="leaveDeleted"
          >
            {{ t('passwords.stale.back') }}
          </UiButton>
        </div>

        <div
          v-if="inTrash"
          class="flex items-center gap-2 rounded-xl bg-muted px-4 py-3 text-sm"
          role="status"
          data-testid="wallet-in-trash"
        >
          <Icon name="lucide:trash-2" class="size-4 shrink-0" />
          <span class="min-w-0 flex-1">{{ t('wallet.inTrashNote') }}</span>
        </div>

        <!-- The hook sits on a plain element: the badge of the UI layer does not forward it. -->
        <div v-if="expired" class="self-start" data-testid="wallet-expired">
          <ShadcnBadge variant="destructive">
            {{ t('passwords.expiredOn', { date: detail.expiresAt }) }}
          </ShadcnBadge>
        </div>

        <WalletBelongsTo
          :detail="detail"
          :readonly="inTrash"
          @changed="onLinkChanged"
        />

        <PasswordsEntryTabs v-model="activeTab" :tabs="TABS">
          <template #details>
            <div class="flex flex-col gap-4">
              <WalletDetails :item-id="itemId" :detail="detail" />
              <WalletLinked :item-id="itemId" />
            </div>
          </template>
          <template #extra>
            <PasswordsViewExtra
              :item-id="itemId"
              :detail="detail"
              pass
              :readonly="inTrash"
              @changed="loadAsync"
            />
          </template>
          <template #history>
            <PasswordsHistoryTab
              :item-id="itemId"
              :updated-at="detail.updatedAt"
              :active="activeTab === 'history'"
              :readonly="inTrash"
              @restored="onRestored"
            />
          </template>
        </PasswordsEntryTabs>
      </template>
    </div>

    <WalletMoveDialog v-model:open="moveOpen" :item-id="itemId" />

    <PasswordsDeleteDialog
      v-model:open="deleteOpen"
      :targets="[{ kind: 'item', id: itemId }]"
      @done="leaveDeleted"
    />
  </div>
</template>
