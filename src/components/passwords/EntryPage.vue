<script setup lang="ts">
/**
 * The place `/entry/:id` (spec 034, research R13): the login's view, or its editor for
 * `/entry/:id?edit` and for `/entry/new`. The path carries the id only, never a title or a value. A
 * card or ticket (spec 092) has its own app, Wallet: its place here shows a notice with a button
 * that opens it there, and no editor of the password manager ever opens for it.
 */
import { entryTypeOf } from '~/lib/passwords/passType'
import { walletEntryPath } from '~/lib/wallet/registry'

const router = useTabRouter()
const store = usePasswordsStore()
const { openApp } = useWmTab()

const itemId = computed(() => router.route.params.id ?? '')
const isNew = computed(() => itemId.value === 'new')
const type = computed(() => entryTypeOf(store.headersById.get(itemId.value)))
/** An entry of a type a newer holzi wrote is shown as such, never opened in an editor (spec 092, FR-042). */
const unknownType = computed(() => type.value === 'unknown')
/** A pass belongs to Wallet. Until the overview is loaded the type of an entry is not known. */
const isPass = computed(() => !isNew.value && type.value === 'pass')
/** Until the overview is loaded the type of an existing entry is not known, so nothing opens for
 * it yet: its view or editor would be the login's, and a pass would show as one. */
const loading = computed(() => !isNew.value && !store.hasLoadedOnce)
const editing = computed(() => router.route.query.edit !== undefined)

function openInWallet() {
  const path = walletEntryPath(itemId.value)
  if (path) openApp('system.wallet', path)
}
</script>

<template>
  <div
    v-if="loading"
    class="flex justify-center py-10 text-muted-foreground"
    role="status"
  >
    <Icon name="lucide:loader-circle" class="size-6 animate-spin" />
  </div>
  <PasswordsUnknownType v-else-if="!isNew && unknownType" />
  <div
    v-else-if="isPass"
    class="mx-auto flex max-w-xl flex-col items-center gap-3 p-8 text-center"
    data-testid="passwords-pass-in-wallet"
  >
    <Icon name="lucide:wallet" class="size-10 text-muted-foreground" />
    <p class="text-muted-foreground">{{ $t('passwords.inWallet.body') }}</p>
    <UiButton
      variant="outline"
      data-testid="passwords-open-in-wallet"
      @click="openInWallet"
    >
      {{ $t('passwords.inWallet.open') }}
    </UiButton>
  </div>
  <PasswordsEntryEditor
    v-else-if="isNew || editing"
    :key="isNew ? 'new' : `edit-${itemId}`"
    :item-id="isNew ? null : itemId"
  />
  <PasswordsEntryView v-else :key="itemId" :item-id="itemId" />
</template>
