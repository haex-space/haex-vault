<script setup lang="ts">
/**
 * The menu button at the end of a row (spec 036, US4, FR-018): the same actions as the right-click
 * menu, for touch screens and anyone without a right mouse button. It shows on hover and focus, and
 * always on narrow windows and devices without hover.
 */
import type { MenuCommand, MenuEntry } from '~/lib/passwords/menus'

defineOptions({ inheritAttrs: false })

const props = defineProps<{
  /** The list, or a function that builds it when the menu opens (a row does not build it on every
   * render of the list). */
  entries: readonly MenuEntry[] | (() => readonly MenuEntry[])
  /** The accessible name, naming the row. */
  label: string
}>()

const emit = defineEmits<{
  run: [command: MenuCommand]
  open: []
}>()

const { t } = useI18n()
const { shortcut } = useShortcutText()

function shown(): readonly MenuEntry[] {
  return typeof props.entries === 'function' ? props.entries() : props.entries
}
</script>

<template>
  <ShadcnDropdownMenu @update:open="(open: boolean) => open && emit('open')">
    <ShadcnDropdownMenuTrigger as-child>
      <button
        type="button"
        class="flex size-8 shrink-0 items-center justify-center rounded text-muted-foreground opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 hover:bg-foreground/10 focus:opacity-100 @max-md:opacity-100 [@media(hover:none)]:opacity-100"
        :aria-label="label"
        tabindex="-1"
        v-bind="$attrs"
      >
        <Icon name="lucide:ellipsis-vertical" class="size-4" />
      </button>
    </ShadcnDropdownMenuTrigger>
    <ShadcnDropdownMenuContent align="end" class="min-w-52">
      <template v-for="(entry, index) in shown()" :key="index">
        <ShadcnDropdownMenuSeparator v-if="'separator' in entry" />
        <ShadcnDropdownMenuItem
          v-else
          :disabled="entry.disabled"
          :data-testid="`passwords-menu-${entry.id}`"
          @select="emit('run', entry.id)"
        >
          {{ t(entry.labelKey) }}
          <ShadcnDropdownMenuShortcut v-if="entry.shortcut">
            {{ shortcut(entry.shortcut) }}
          </ShadcnDropdownMenuShortcut>
        </ShadcnDropdownMenuItem>
      </template>
    </ShadcnDropdownMenuContent>
  </ShadcnDropdownMenu>
</template>
