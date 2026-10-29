<script setup lang="ts">
/**
 * One accordion of the install dialog (spec 017, FR-067): the permissions of one kind, closed at
 * first. Its header names the kind and how many of its permissions are ticked, and carries a
 * warning triangle while the group holds a wildcard, so the warning shows closed as well. The
 * rows (`<li>`) are the slot.
 */
defineProps<{
  label: string
  granted: number
  total: number
  /** The group holds a wildcard that allows everything of its kind. */
  wide?: boolean
}>()
const { t } = useI18n()
</script>

<template>
  <details
    class="group overflow-hidden rounded-xl bg-muted"
    data-testid="extensions-install-group"
  >
    <summary
      class="flex cursor-pointer list-none items-center gap-3 px-4 py-3 outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
    >
      <span class="min-w-0 flex-1 text-sm font-medium">{{ label }}</span>
      <span
        v-if="wide"
        class="flex shrink-0 text-warning"
        :title="t('extensions.install.groupWide')"
        data-testid="extensions-install-group-wide"
      >
        <Icon name="lucide:triangle-alert" class="size-4" :aria-hidden="true" />
        <span class="sr-only">{{ t('extensions.install.groupWide') }}</span>
      </span>
      <span class="shrink-0 text-xs text-muted-foreground">{{
        t('extensions.install.groupCount', { granted, total })
      }}</span>
      <Icon
        name="lucide:chevron-down"
        class="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180"
        :aria-hidden="true"
      />
    </summary>
    <ul
      class="flex flex-col divide-y divide-background border-t border-background"
    >
      <slot />
    </ul>
  </details>
</template>
