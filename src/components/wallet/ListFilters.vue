<script setup lang="ts">
/**
 * The filters of Wallet's list beside the search (spec 092, FR-010, FR-021): by kind (a place,
 * `?kind=`), the sort (date or title) and "Hide expired" (view settings kept while the vault is
 * open). The button is marked while anything filters the list or the sort is not the default.
 */
import { PASS_KIND_ICONS, PASS_KINDS } from '~/lib/passwords/passType'

const { t } = useI18n()
const router = useTabRouter()
const view = useWalletListViewStore()

const kind = computed(() => {
  const value = router.route.query.kind
  return PASS_KINDS.find((candidate) => candidate === value) ?? null
})
const active = computed(
  () => kind.value !== null || view.hideExpired || view.sort !== 'date',
)

function chooseKind(value: string | null) {
  router.setQuery({ kind: value })
}
</script>

<template>
  <ShadcnDropdownMenu>
    <ShadcnDropdownMenuTrigger as-child>
      <UiButton
        :variant="active ? 'secondary' : 'ghost'"
        size="icon"
        class="shrink-0"
        :aria-label="t('wallet.filters.label')"
        :tooltip="t('wallet.filters.label')"
        data-testid="wallet-filters"
      >
        <Icon name="lucide:sliders-horizontal" class="size-4" />
      </UiButton>
    </ShadcnDropdownMenuTrigger>
    <ShadcnDropdownMenuContent align="end" class="max-h-96 overflow-y-auto">
      <ShadcnDropdownMenuLabel>{{
        t('wallet.filters.sort')
      }}</ShadcnDropdownMenuLabel>
      <ShadcnDropdownMenuItem
        v-for="mode in ['date', 'title'] as const"
        :key="mode"
        class="gap-2"
        :class="view.sort === mode ? 'bg-accent font-medium' : ''"
        :data-testid="`wallet-sort-${mode}`"
        @select="view.sort = mode"
      >
        <Icon
          :name="mode === 'date' ? 'lucide:calendar' : 'lucide:arrow-down-a-z'"
          class="size-4 shrink-0"
        />
        {{ t(`wallet.filters.sorts.${mode}`) }}
      </ShadcnDropdownMenuItem>
      <ShadcnDropdownMenuSeparator />
      <ShadcnDropdownMenuLabel>{{
        t('wallet.filters.kind')
      }}</ShadcnDropdownMenuLabel>
      <ShadcnDropdownMenuItem
        class="gap-2"
        :class="kind === null ? 'bg-accent font-medium' : ''"
        data-testid="wallet-kind-all"
        @select="chooseKind(null)"
      >
        <Icon name="lucide:wallet" class="size-4 shrink-0" />
        {{ t('wallet.filters.allKinds') }}
      </ShadcnDropdownMenuItem>
      <ShadcnDropdownMenuItem
        v-for="candidate in PASS_KINDS"
        :key="candidate"
        class="gap-2"
        :class="kind === candidate ? 'bg-accent font-medium' : ''"
        :data-testid="`wallet-kind-${candidate}`"
        @select="chooseKind(candidate)"
      >
        <Icon :name="PASS_KIND_ICONS[candidate]" class="size-4 shrink-0" />
        {{ t(`passwords.kinds.${candidate}`) }}
      </ShadcnDropdownMenuItem>
      <ShadcnDropdownMenuSeparator />
      <ShadcnDropdownMenuItem
        class="gap-2"
        :class="view.hideExpired ? 'bg-accent font-medium' : ''"
        data-testid="wallet-hide-expired"
        :aria-pressed="view.hideExpired"
        @select="view.hideExpired = !view.hideExpired"
      >
        <Icon name="lucide:calendar-x" class="size-4 shrink-0" />
        {{ t('wallet.filters.hideExpired') }}
      </ShadcnDropdownMenuItem>
    </ShadcnDropdownMenuContent>
  </ShadcnDropdownMenu>
</template>
