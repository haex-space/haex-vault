<script setup lang="ts">
/**
 * The first row of the Wallet frame (spec 092, Phase W): the search field bound to `?q=` of the
 * place and "New", which opens the editor of a new pass. The search text is the only free text a
 * place carries; it is never a secret (the search looks at title, address and tag names only).
 * While a pass is open, the search leaves the row.
 */
import { Search } from '@lucide/vue'
import { NEW_PASS } from '~/lib/wallet/registry'

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const router = useTabRouter()

/** A pass, the trash or another detail place is open: search and "New" leave the row. */
const passOpen = computed(() => router.route.path !== '/')

const query = ref(router.route.query.q ?? '')
// A history step to another place or another search puts its own text into the field.
watch(
  () => router.route.query.q ?? '',
  (value) => {
    if (value !== query.value) query.value = value
  },
)

function onUpdate(value: string | number | null | undefined) {
  query.value = String(value ?? '')
  router.setQuery({ q: query.value.trim() ? query.value : null })
}

function clear() {
  onUpdate('')
}

function newPass() {
  void router.push(`/entry/${NEW_PASS}`)
}
</script>

<template>
  <div class="flex items-center gap-1">
    <div v-if="passOpen" class="flex-1" />
    <UiInput
      v-else
      :model-value="query"
      type="search"
      :placeholder="t('wallet.search.placeholder')"
      :aria-label="t('wallet.search.label')"
      :labels="fieldLabels.input.value"
      :prepend-icon="Search"
      clearable
      class="min-w-0 flex-1 [&_input::-webkit-search-cancel-button]:hidden"
      data-testid="wallet-search"
      @update:model-value="onUpdate"
      @keydown.esc.prevent.stop="clear"
    />
    <UiButton
      v-if="!passOpen"
      size="icon"
      class="shrink-0"
      :aria-label="t('wallet.new')"
      :tooltip="t('wallet.new')"
      data-testid="wallet-new"
      @click="newPass"
    >
      <Icon name="lucide:plus" class="size-4" />
    </UiButton>
    <ShadcnDropdownMenu v-if="!passOpen">
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          variant="ghost"
          size="icon"
          class="shrink-0"
          :aria-label="t('wallet.menu')"
          :tooltip="t('wallet.menu')"
          data-testid="wallet-menu"
        >
          <Icon name="lucide:menu" class="size-4" />
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="end">
        <ShadcnDropdownMenuItem
          class="gap-2"
          data-testid="wallet-open-trash"
          @select="router.push('/trash')"
        >
          <Icon name="lucide:trash-2" class="size-4" />
          {{ t('wallet.trash') }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
  </div>
</template>
