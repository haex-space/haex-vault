<script setup lang="ts">
/**
 * One clickable row of a sidebar (files, contacts, passwords, chat): an icon, a label that
 * truncates, and an optional `trailing` slot (a count, a note such as "read only"). The current
 * place is `active`. It is a plain `<button>` on purpose (a row is not a button to look at), so
 * attributes and listeners (`data-testid`, `@click`, `aria-expanded`, drag events) fall through
 * to it, and the small-screen rule for tap targets in `tailwind.css` reaches it like any other.
 * A row beside another control passes `class="flex-1"`.
 */
defineProps<{
  /** Icon name, e.g. `lucide:key-round`. */
  icon?: string
  /** Extra classes for the icon (a colour, a spin while busy). */
  iconClass?: string | string[] | Record<string, boolean>
  active?: boolean
}>()
</script>

<template>
  <button
    type="button"
    class="flex min-h-9 w-full min-w-0 items-center gap-2 rounded-lg px-3 py-1.5 text-left enabled:hover:bg-foreground/5 disabled:cursor-not-allowed disabled:opacity-60"
    :class="active ? 'bg-foreground/10 font-medium' : ''"
  >
    <slot name="icon">
      <Icon
        v-if="icon"
        :name="icon"
        class="size-4 shrink-0"
        :class="iconClass"
      />
    </slot>
    <span class="min-w-0 flex-1 truncate"><slot /></span>
    <slot name="trailing" />
  </button>
</template>
