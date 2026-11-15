<script setup lang="ts">
/**
 * The sidebar of an app frame, the same in every app (passwords, contacts, wallet, files,
 * settings): below `@2xl` it lies over the content and slides in from its side, from `@2xl` it is a
 * column beside the content that is hidden by its width. `open` and `wideHidden` come from
 * `useSidebarFrame`; the frame passes `id`, `aria-label` and `data-testid` as attributes. The
 * element it renders is what `useSidebarFrame` reads "wide" from, so give the composable
 * `() => panel.value?.$el`. The window's overflow sidebar on the right is this same component with
 * `side="right"` (`HeaderOverflow.vue`).
 */
import { sidebarPanelClass } from '~/lib/wm/slide'

const { side = 'left' } = defineProps<{
  open: boolean
  wideHidden: boolean
  side?: 'left' | 'right'
}>()
</script>

<template>
  <nav :class="sidebarPanelClass(open, wideHidden, side)">
    <slot />
  </nav>
</template>
