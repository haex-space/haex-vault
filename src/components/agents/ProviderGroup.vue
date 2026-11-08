<script setup lang="ts">
/**
 * The models of one provider as an accordion row of a `SettingsGroup` (spec 083): closed at first,
 * with the provider's name and how many models it holds, so a long list of models stays short. The
 * rows (`SettingsRow`) are the slot. `open` keeps a group open that holds what the user is
 * working on.
 */
defineProps<{
  label: string
  count: number
  open?: boolean
}>()
</script>

<template>
  <li data-testid="agent-provider-group">
    <details class="group" :open="open">
      <summary
        class="flex min-h-14 cursor-pointer list-none items-center gap-4 px-4 py-3 outline-none hover:bg-foreground/5 focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
      >
        <Icon
          name="lucide:layers"
          class="size-5 shrink-0"
          :aria-hidden="true"
        />
        <span class="min-w-0 flex-1 font-medium">{{ label }}</span>
        <span class="shrink-0 text-sm text-muted-foreground">{{ count }}</span>
        <Icon
          name="lucide:chevron-down"
          class="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180"
          :aria-hidden="true"
        />
      </summary>
      <ul
        class="flex flex-col divide-y divide-background border-t border-background bg-foreground/5"
      >
        <slot />
      </ul>
    </details>
  </li>
</template>
