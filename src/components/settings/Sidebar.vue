<script setup lang="ts">
/**
 * Category sidebar of the settings (spec 023-settings-app, FR-001, FR-004, FR-010, FR-023): the
 * categories in registry order with the current one highlighted, or — while a search runs — its
 * hits with their path. Choosing either emits the target's own location, so a category entered
 * from the sidebar starts at its overview. Where and how wide the sidebar is, the frame
 * (`apps/SettingsApp.vue`) decides; the content keeps its width while the frame slides it, so
 * nothing reflows during the transition.
 */
import { categoryOf, SETTINGS_CATEGORIES } from '~/lib/settings/registry'
import type { SettingsSearchHit } from '~/lib/settings/search'

defineProps<{
  /** `null` while no search runs. */
  hits: SettingsSearchHit[] | null
}>()

const emit = defineEmits<{
  select: [path: string]
}>()

const { t } = useI18n()
const router = useTabRouter()

const active = computed(() => categoryOf(router.route.path))
const list = useTemplateRef<HTMLElement>('list')

function iconOf(hit: SettingsSearchHit): string {
  return (
    hit.location.icon ??
    SETTINGS_CATEGORIES.find((c) => c.id === hit.location.category)?.icon ??
    'lucide:settings-2'
  )
}

const ITEM =
  'flex w-full items-center gap-3 rounded-md p-3 text-left text-base font-medium hover:bg-foreground/5'

defineExpose({
  focusActive() {
    list.value
      ?.querySelector<HTMLElement>('[aria-current="page"], button')
      ?.focus()
  },
})
</script>

<template>
  <div class="h-full p-2 @2xl:pr-0">
    <div
      ref="list"
      class="h-full min-w-60 overflow-y-auto rounded-xl bg-muted p-2"
    >
      <template v-if="hits">
        <ul
          v-if="hits.length > 0"
          class="flex flex-col gap-1"
          :aria-label="t('settings.search.label')"
        >
          <li v-for="hit in hits" :key="`${hit.location.id}:${hit.label}`">
            <button
              type="button"
              :class="ITEM"
              data-testid="settings-search-hit"
              :data-location="hit.location.id"
              @click="emit('select', hit.path)"
            >
              <Icon :name="iconOf(hit)" class="size-6 shrink-0" />
              <span class="flex min-w-0 flex-col">
                <span class="truncate">{{ hit.label }}</span>
                <span
                  v-if="hit.trail.length > 0"
                  class="truncate text-xs text-muted-foreground"
                >
                  {{ hit.trail.join(' › ') }}
                </span>
              </span>
            </button>
          </li>
        </ul>
        <p v-else class="px-3 py-2 text-sm text-muted-foreground" role="status">
          {{ t('settings.search.noResults') }}
        </p>
      </template>

      <ul v-else class="flex flex-col gap-1">
        <li v-for="category in SETTINGS_CATEGORIES" :key="category.id">
          <button
            type="button"
            class="whitespace-nowrap"
            :class="[
              ITEM,
              active === category.id
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : '',
            ]"
            :aria-current="active === category.id ? 'page' : undefined"
            :data-testid="`settings-category-${category.id}`"
            @click="emit('select', category.path)"
          >
            <Icon :name="category.icon" class="size-6 shrink-0" />
            <span class="truncate">{{ t(category.titleKey) }}</span>
          </button>
        </li>
      </ul>
    </div>
  </div>
</template>
