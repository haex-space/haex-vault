<script setup lang="ts">
/**
 * The place `/entry/:id` of Wallet (spec 092, Phase W): the view of a pass, or its editor for
 * `/entry/:id?edit` and for `/entry/new`. The path carries the id only, never a title or a code. An
 * id that belongs to a login is not a pass: Wallet says so and offers the password manager.
 */
import { entryTypeOf } from '~/lib/passwords/passType'
import { NEW_PASS } from '~/lib/wallet/registry'

const router = useTabRouter()
const store = usePasswordsStore()
const { openApp } = useWmTab()
const { t } = useI18n()

const itemId = computed(() => router.route.params.id ?? '')
const isNew = computed(() => itemId.value === NEW_PASS)
const type = computed(() => entryTypeOf(store.headersById.get(itemId.value)))
const editing = computed(() => router.route.query.edit !== undefined)
/** The overview is loaded and the entry in it is a login. */
const isLogin = computed(
  () => !isNew.value && store.hasLoadedOnce && type.value === 'login',
)
</script>

<template>
  <PasswordsUnknownType v-if="!isNew && type === 'unknown'" />
  <div
    v-else-if="isLogin"
    class="mx-auto flex max-w-xl flex-col items-center gap-3 p-8 text-center"
    data-testid="wallet-not-a-pass"
  >
    <Icon name="lucide:key-round" class="size-10 text-muted-foreground" />
    <p class="text-muted-foreground">{{ t('wallet.notAPass') }}</p>
    <UiButton
      variant="outline"
      data-testid="wallet-open-passwords"
      @click="openApp('system.passwords', `/entry/${itemId}`)"
    >
      {{ t('wallet.openInPasswords') }}
    </UiButton>
  </div>
  <WalletEditor
    v-else-if="isNew || (editing && store.hasLoadedOnce)"
    :key="isNew ? 'new-pass' : `edit-pass-${itemId}`"
    :item-id="isNew ? null : itemId"
  />
  <WalletEntryView v-else :key="itemId" :item-id="itemId" />
</template>
