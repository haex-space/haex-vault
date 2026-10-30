<script setup lang="ts">
/**
 * The marketplaces of this vault (spec 069 US4, FR-020 to FR-024): Settings → Erweiterungen →
 * Marketplaces. Every row is equal — the public one the vault starts with is no special row and
 * carries no badge — and each can be switched on or off (it applies at once), moved up or down
 * (the order is the order of the catalog) and edited. The list follows the table, so a change
 * from another device shows here without reopening.
 */
import { invoke } from '@tauri-apps/api/core'
import type { MarketplaceView } from '@bindings/MarketplaceView'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()

const markets = ref<MarketplaceView[]>([])
const loaded = ref(false)
const failure = ref<string | null>(null)

async function loadAsync() {
  try {
    markets.value = await invoke<MarketplaceView[]>('marketplace_list')
    loaded.value = true
  } catch (error) {
    failure.value = errString(error)
  }
}

onMounted(() => void loadAsync())
onVaultTablesChanged(['haex_marketplaces'], loadAsync)

async function runAsync(action: () => Promise<unknown>) {
  failure.value = null
  try {
    await action()
  } catch (error) {
    failure.value = errString(error)
  }
  await loadAsync()
}

function setEnabledAsync(market: MarketplaceView, enabled: boolean) {
  return runAsync(() =>
    invoke('marketplace_set_enabled', { id: market.id, enabled }),
  )
}

/** Moves the marketplace one place and writes the complete order. */
function moveAsync(index: number, by: -1 | 1) {
  const ids = markets.value.map((market) => market.id)
  const target = index + by
  if (target < 0 || target >= ids.length) return Promise.resolve()
  ;[ids[index], ids[target]] = [ids[target]!, ids[index]!]
  return runAsync(() => invoke('marketplace_reorder', { ids }))
}

function credentialsLine(market: MarketplaceView): string | null {
  if (market.credentials === 'missing')
    return t('settings.extensions.marketplaces.credentials.missing')
  if (market.credentials === 'syncing')
    return t('settings.extensions.marketplaces.credentials.syncing')
  return null
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="settings-marketplaces">
    <SettingsGroup>
      <SettingsRow
        icon="lucide:plus"
        :title="t('settings.extensions.marketplaces.add')"
        :description="t('settings.extensions.marketplaces.addHint')"
        to="/extensions/marketplaces/new"
        data-testid="settings-marketplace-add"
      />
    </SettingsGroup>

    <SettingsGroup
      v-if="markets.length > 0"
      :label="t('settings.locations.extensions.marketplaces.title')"
    >
      <SettingsRow
        v-for="(market, index) in markets"
        :key="market.id"
        icon="lucide:store"
        :title="market.name"
        :data-testid="`settings-marketplace-row-${market.id}`"
      >
        <template #description>
          <span class="block break-all">{{ market.baseUrl }}</span>
          <span
            v-if="credentialsLine(market)"
            class="block text-destructive"
            :data-testid="`settings-marketplace-credentials-${market.id}`"
          >
            {{ credentialsLine(market) }}
          </span>
        </template>
        <ShadcnSwitch
          :model-value="market.enabled"
          :aria-label="
            t('settings.extensions.marketplaces.toggle', { name: market.name })
          "
          :data-testid="`settings-marketplace-toggle-${market.id}`"
          @update:model-value="setEnabledAsync(market, $event === true)"
        />
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          :disabled="index === 0"
          :aria-label="
            t('settings.extensions.marketplaces.moveUp', { name: market.name })
          "
          :tooltip="
            t('settings.extensions.marketplaces.moveUp', { name: market.name })
          "
          :data-testid="`settings-marketplace-up-${market.id}`"
          @click="moveAsync(index, -1)"
        >
          <Icon name="lucide:arrow-up" class="size-4" />
        </UiButton>
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          :disabled="index === markets.length - 1"
          :aria-label="
            t('settings.extensions.marketplaces.moveDown', {
              name: market.name,
            })
          "
          :tooltip="
            t('settings.extensions.marketplaces.moveDown', {
              name: market.name,
            })
          "
          :data-testid="`settings-marketplace-down-${market.id}`"
          @click="moveAsync(index, 1)"
        >
          <Icon name="lucide:arrow-down" class="size-4" />
        </UiButton>
        <UiButton
          type="button"
          variant="outline"
          size="sm"
          :data-testid="`settings-marketplace-edit-${market.id}`"
          @click="router.push(`/extensions/marketplaces/${market.id}`)"
        >
          {{ t('settings.extensions.marketplaces.edit') }}
        </UiButton>
      </SettingsRow>
    </SettingsGroup>
    <p
      v-else-if="loaded"
      class="px-1 text-sm text-muted-foreground"
      data-testid="settings-marketplaces-empty"
    >
      {{ t('settings.extensions.marketplaces.none') }}
    </p>

    <p v-if="failure" class="px-1 text-sm text-destructive" role="alert">
      {{ failure }}
    </p>
  </section>
</template>
