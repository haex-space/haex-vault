<script setup lang="ts">
/**
 * The details of one entry (spec 069, FR-005, FR-006): description, publisher with the key the
 * marketplace names, category, tags, the marketplace it comes from and every version with its
 * change note and whether it fits this HaexVault. Everything the marketplace sent is plain text.
 * A chosen older version goes through the same preview, with the downgrade confirmation of
 * spec 017. The route is `/:slug?source=<id>`; "Zurück" returns to the catalog as it was.
 */
import { invoke } from '@tauri-apps/api/core'
import type { CatalogDetail } from '@bindings/CatalogDetail'
import { selectable } from '~/lib/marketplace/tileState'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const extensions = useExtensionsStore()
const { busy, startAsync } = useMarketplaceInstall()

const slug = computed(() => router.route.params.slug ?? '')
const sourceId = computed(() => router.route.query.source ?? '')

/** The installed extension of the entry's name, which "Entfernen" removes. */
const installedExtension = computed(() => {
  const name = detail.value?.entry.name
  return name === undefined
    ? undefined
    : extensions.list.find((e) => e.name === name && e.state === 'installed')
})
const removing = ref(false)

const detail = ref<CatalogDetail | null>(null)
const failure = ref<string | null>(null)
const loading = ref(false)

let sequence = 0
async function loadAsync(quiet = false) {
  const mine = ++sequence
  if (!quiet) loading.value = true
  try {
    const loaded = await invoke<CatalogDetail>('marketplace_details', {
      sourceId: sourceId.value,
      slug: slug.value,
    })
    if (mine !== sequence) return
    detail.value = loaded
    failure.value = null
  } catch (error) {
    if (mine !== sequence) return
    detail.value = null
    failure.value = errString(error)
  } finally {
    if (mine === sequence) loading.value = false
  }
}

watch([slug, sourceId], () => void loadAsync(), { immediate: true })
// An install, an update or a removal (here, in the settings or on another device) changes the mark.
watch(
  () =>
    extensions.list
      .map((e) => `${e.name}@${e.version ?? ''}:${e.enabled}`)
      .join('|'),
  () => void loadAsync(true),
)

function back() {
  if (router.canGoBack) router.back()
  else void router.push('/')
}

function install(version: string) {
  const entry = detail.value?.entry
  if (!entry) return
  void startAsync({
    sourceId: entry.source.id,
    slug: entry.slug,
    name: entry.name,
    version,
  })
}
</script>

<template>
  <div class="flex flex-col gap-4 p-3" data-testid="marketplace-details">
    <div>
      <UiButton
        variant="ghost"
        size="sm"
        data-testid="marketplace-back"
        @click="back"
      >
        <Icon name="lucide:arrow-left" class="size-4" />
        {{ t('marketplace.back') }}
      </UiButton>
    </div>

    <p
      v-if="loading && !detail"
      class="py-10 text-center text-sm text-muted-foreground"
      role="status"
    >
      {{ t('marketplace.loading') }}
    </p>

    <p
      v-else-if="failure && !detail"
      class="rounded-xl bg-destructive/10 px-4 py-3 text-sm text-destructive"
      role="alert"
      data-testid="marketplace-details-error"
    >
      {{ t('marketplace.details.unavailable') }} {{ failure }}
    </p>

    <template v-else-if="detail">
      <header class="flex items-start gap-4">
        <MarketplaceEntryIcon
          :source-id="detail.entry.source.id"
          :url="detail.entry.iconUrl"
          size="lg"
        />
        <div class="flex min-w-0 flex-1 flex-col gap-1">
          <h2 class="text-xl font-semibold">{{ detail.entry.name }}</h2>
          <p class="text-sm text-muted-foreground">
            {{ detail.entry.shortDescription }}
          </p>
        </div>
      </header>

      <div class="flex flex-wrap items-center gap-2">
        <MarketplaceTileAction :entry="detail.entry" class="flex-1" />
        <UiButton
          v-if="installedExtension"
          variant="destructive"
          size="sm"
          :data-testid="`marketplace-remove-${detail.entry.source.id}-${detail.entry.slug}`"
          @click="removing = true"
        >
          {{ t('marketplace.remove') }}
        </UiButton>
      </div>
      <ExtensionsRemoveDialog
        v-if="installedExtension"
        v-model:open="removing"
        :extension="installedExtension"
      />

      <section class="flex flex-col gap-1">
        <h3 class="text-sm font-semibold">
          {{ t('marketplace.details.description') }}
        </h3>
        <p class="whitespace-pre-line text-sm">{{ detail.description }}</p>
      </section>

      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        <template v-if="detail.entry.publisher">
          <dt class="text-muted-foreground">
            {{ t('marketplace.details.publisher') }}
          </dt>
          <dd>
            {{ detail.entry.publisher.displayName }}
            <span v-if="detail.entry.publisher.verified">
              · {{ t('marketplace.tile.verified') }}
            </span>
            <span
              v-if="detail.publisherFingerprint"
              class="block font-mono text-xs text-muted-foreground"
            >
              {{
                t('marketplace.details.publisherKey', {
                  fingerprint: detail.publisherFingerprint,
                })
              }}
            </span>
          </dd>
        </template>
        <template v-if="detail.entry.category">
          <dt class="text-muted-foreground">
            {{ t('marketplace.details.category') }}
          </dt>
          <dd>{{ detail.entry.category.name }}</dd>
        </template>
        <template v-if="detail.entry.tags.length > 0">
          <dt class="text-muted-foreground">
            {{ t('marketplace.details.tags') }}
          </dt>
          <dd>{{ detail.entry.tags.join(', ') }}</dd>
        </template>
        <dt class="text-muted-foreground">
          {{ t('marketplace.details.source') }}
        </dt>
        <dd data-testid="marketplace-details-source">
          {{ detail.entry.source.name }}
        </dd>
      </dl>

      <section class="flex flex-col gap-2">
        <h3 class="text-sm font-semibold">
          {{ t('marketplace.details.versions') }}
        </h3>
        <ul class="flex flex-col gap-2">
          <li
            v-for="version in detail.entry.versions"
            :key="version.version"
            class="flex flex-wrap items-start justify-between gap-2 rounded-xl border bg-background px-4 py-3"
            :data-testid="`marketplace-version-${version.version}`"
          >
            <div class="flex min-w-0 flex-1 flex-col gap-0.5">
              <span class="text-sm font-medium">{{
                t('marketplace.details.version', { version: version.version })
              }}</span>
              <span class="text-xs text-muted-foreground">
                {{
                  version.compatible
                    ? t('marketplace.details.compatible')
                    : t(
                        `marketplace.blocked.${version.incompatibleReason ?? 'no_versions'}`,
                        { bound: version.bound },
                      )
                }}
              </span>
              <span class="whitespace-pre-line text-sm">{{
                version.changelog ?? t('marketplace.details.noChangelog')
              }}</span>
            </div>
            <UiButton
              variant="outline"
              size="sm"
              :disabled="busy || !selectable(version)"
              :data-testid="`marketplace-install-version-${version.version}`"
              @click="install(version.version)"
            >
              {{ t('marketplace.details.choose') }}
            </UiButton>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>
