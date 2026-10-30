<script setup lang="ts">
/**
 * One entry of the catalog (spec 069, FR-001, FR-006): icon, name, short description, publisher
 * (the marketplace's own "geprüft" is only displayed, FR-005), rating, downloads and the
 * marketplace it comes from. Everything a marketplace sent is shown as plain text. The tile opens
 * the details; the action button beside it installs or updates.
 */
import type { CatalogEntry } from '@bindings/CatalogEntry'
import { detailsPath } from '~/lib/marketplace/registry'

const props = defineProps<{ entry: CatalogEntry }>()
const { t, n } = useI18n()
const router = useTabRouter()

const rating = computed(() =>
  props.entry.averageRating === undefined
    ? t('marketplace.tile.noRating')
    : t('marketplace.tile.rating', {
        rating: props.entry.averageRating.toFixed(1),
        count: props.entry.reviewCount,
      }),
)

function open() {
  void router.push(detailsPath(props.entry.source.id, props.entry.slug))
}
</script>

<template>
  <li
    class="flex flex-col gap-3 rounded-xl border bg-background p-4"
    :data-testid="`marketplace-tile-${entry.source.id}-${entry.slug}`"
  >
    <button
      type="button"
      class="flex min-h-11 items-start gap-3 rounded-lg text-left focus-visible:outline-2 focus-visible:outline-offset-2"
      :aria-label="t('marketplace.tile.open', { name: entry.name })"
      @click="open"
    >
      <MarketplaceEntryIcon :source-id="entry.source.id" :url="entry.iconUrl" />
      <span class="flex min-w-0 flex-1 flex-col gap-0.5">
        <span class="truncate font-semibold">{{ entry.name }}</span>
        <span class="line-clamp-2 text-sm text-muted-foreground">{{
          entry.shortDescription
        }}</span>
      </span>
    </button>
    <p
      v-if="entry.publisher"
      class="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground"
    >
      <span>{{
        t('marketplace.tile.by', { publisher: entry.publisher.displayName })
      }}</span>
      <span v-if="entry.publisher.verified" class="flex items-center gap-1">
        <Icon name="lucide:badge-check" class="size-3.5" />
        {{ t('marketplace.tile.verified') }}
      </span>
    </p>
    <p class="text-xs text-muted-foreground">
      {{ rating }} ·
      {{ t('marketplace.tile.downloads', { count: n(entry.totalDownloads) }) }}
    </p>
    <p
      class="text-xs text-muted-foreground"
      :data-testid="`marketplace-tile-source-${entry.source.id}-${entry.slug}`"
    >
      {{ t('marketplace.tile.from', { source: entry.source.name }) }}
    </p>
    <MarketplaceTileAction :entry="entry" class="mt-auto" />
  </li>
</template>
