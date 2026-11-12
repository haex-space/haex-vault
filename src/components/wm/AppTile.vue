<script setup lang="ts">
/**
 * One app as a tile, in the look of COSMIC's app library (spec 091-tab-switcher, FR-003,
 * contracts/app-tiles.md): the icon large, the name below it in up to two lines, no frame, a soft
 * hover background. An attention dot sits at the icon's corner (a tab of this app waits on a reply,
 * anywhere — spec 015 FR-015); an extension that cannot open on this device shows the reason as a
 * third line, and with `open` a single-instance app that already has a tab says so.
 *
 * An unavailable tile is `aria-disabled`, not `disabled`: it stays reachable by the arrow keys of
 * `AppTiles.vue` and reads its reason aloud, and a press on it does nothing.
 */
import type { AppDefinition } from '~/lib/wm/apps'

const props = defineProps<{
  app: AppDefinition
  /** Say that this single-instance app already has a tab (the "+" of a window, not the launcher). */
  open?: boolean
}>()

const emit = defineEmits<{ pick: [appId: string] }>()

const wm = useWindowManagerStore()
const { t } = useI18n()

const title = computed(() => props.app.title ?? t(props.app.titleKey))
const unavailable = computed(() => props.app.unavailableKey !== undefined)

/** A press on an unavailable tile does nothing; the click of a scroll gesture never arrives. */
function press() {
  if (!unavailable.value) emit('pick', props.app.id)
}
</script>

<template>
  <button
    type="button"
    class="group relative flex min-w-0 flex-col items-center gap-2 rounded-xl p-3 text-center text-sm text-foreground hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring aria-disabled:cursor-not-allowed aria-disabled:opacity-60 aria-disabled:hover:bg-transparent"
    :data-testid="app.id === 'system.chat' ? 'open-chat' : undefined"
    :data-app-id="app.id"
    :title="title"
    :aria-disabled="unavailable || undefined"
    @click="press"
  >
    <span class="relative flex size-14 shrink-0 items-center justify-center">
      <img
        v-if="app.iconUrl"
        :src="app.iconUrl"
        alt=""
        class="size-14 object-contain"
      />
      <Icon v-else :name="app.icon" class="size-10" :aria-hidden="true" />
      <span
        v-if="wm.appHasAttention(app.id)"
        class="absolute right-0 top-0 size-2.5 rounded-full bg-warning"
        role="img"
        :aria-label="t('wm.attention')"
      />
    </span>
    <span class="line-clamp-2 w-full break-words leading-tight">{{
      title
    }}</span>
    <span
      v-if="app.unavailableKey"
      class="line-clamp-2 w-full text-xs leading-tight text-muted-foreground"
    >
      {{ t(app.unavailableKey) }}
    </span>
    <span
      v-else-if="open"
      class="w-full truncate text-xs leading-tight text-muted-foreground"
    >
      {{ t('wm.launcher.alreadyOpen') }}
    </span>
  </button>
</template>
