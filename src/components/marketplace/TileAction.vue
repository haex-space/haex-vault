<script setup lang="ts">
/**
 * What an entry offers (spec 069, FR-004, FR-018) as the state mark and the one button, shared by
 * the tile and the details: "Installieren", "Aktualisieren", "Installiert (x)" or the reason
 * nothing can be installed. The state is not shown by color alone.
 */
import type { CatalogEntry } from '@bindings/CatalogEntry'
import { tileAction } from '~/lib/marketplace/tileState'

const props = defineProps<{ entry: CatalogEntry }>()
const { t } = useI18n()
const { busy, startAsync } = useMarketplaceInstall()

const action = computed(() => tileAction(props.entry))
const id = computed(() => `${props.entry.source.id}-${props.entry.slug}`)

function install(version: string) {
  void startAsync({
    sourceId: props.entry.source.id,
    slug: props.entry.slug,
    name: props.entry.name,
    version,
  })
}
</script>

<template>
  <div class="flex flex-wrap items-center justify-between gap-2">
    <p
      v-if="action.kind === 'installed'"
      class="flex items-center gap-1 text-sm"
      :data-testid="`marketplace-installed-${id}`"
    >
      <Icon name="lucide:check" class="size-4" />
      {{ t('marketplace.installed', { version: action.version }) }}
    </p>
    <template v-else-if="action.kind === 'update'">
      <p class="flex items-center gap-1 text-sm">
        <Icon name="lucide:arrow-up-circle" class="size-4" />
        {{
          t('marketplace.updateAvailable', {
            from: action.from,
            to: action.to,
          })
        }}
      </p>
      <UiButton
        size="sm"
        :disabled="busy"
        :data-testid="`marketplace-update-${id}`"
        @click="install(action.to)"
      >
        {{ t('marketplace.update') }}
      </UiButton>
    </template>
    <UiButton
      v-else-if="action.kind === 'install'"
      size="sm"
      :disabled="busy"
      :data-testid="`marketplace-install-${id}`"
      @click="install(action.version)"
    >
      {{ t('marketplace.install') }}
    </UiButton>
    <template v-else>
      <p
        class="text-sm text-muted-foreground"
        :data-testid="`marketplace-blocked-${id}`"
      >
        {{ t(`marketplace.blocked.${action.reason}`, { bound: action.bound }) }}
      </p>
      <UiButton size="sm" disabled :data-testid="`marketplace-install-${id}`">
        {{ t('marketplace.install') }}
      </UiButton>
    </template>
  </div>
</template>
