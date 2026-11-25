<script setup lang="ts">
/**
 * How often a tag is used, per app (spec 100, FR-015): the icon of the password manager with the
 * number of its entries, the icon of Wallet with the number of its passes. Tags are one namespace
 * for both apps, so one combined number would not match what either app's filter shows. The icons
 * come from the app registry, the very images of the dock, launcher and tabs, so they look the same
 * here whatever icon set is chosen.
 */
import type { TagUsage } from '~/lib/passwords/scope'
import { getAppDefinition } from '~/lib/wm/apps'

const props = defineProps<{ usage: TagUsage }>()

const { t } = useI18n()

const entries = computed(() => [
  {
    key: 'login',
    app: getAppDefinition('system.passwords'),
    count: props.usage.login,
  },
  {
    key: 'pass',
    app: getAppDefinition('system.wallet'),
    count: props.usage.pass,
  },
])
</script>

<template>
  <span class="inline-flex shrink-0 items-center gap-3 text-sm tabular-nums">
    <span
      v-for="entry in entries"
      :key="entry.key"
      role="img"
      :aria-label="
        t(
          `passwords.tags.usage.${entry.key}`,
          { count: entry.count },
          entry.count,
        )
      "
      class="inline-flex items-center gap-1"
      :class="entry.count === 0 && 'text-muted-foreground opacity-60'"
      :data-testid="`passwords-tag-usage-${entry.key}`"
    >
      <WmTabIcon
        :icon="entry.app?.icon"
        :icon-url="entry.app?.iconUrl"
        class="size-4"
      />
      <span aria-hidden="true">{{ entry.count }}</span>
    </span>
  </span>
</template>
