<script setup lang="ts">
/**
 * The sidebar of an app frame, the same in every app (passwords, contacts, wallet, files,
 * settings): below `@2xl` it lies over the content and slides in from the left, from `@2xl` it is a
 * column beside the content that is hidden by its width. `open` and `wideHidden` come from
 * `useSidebarFrame`; the frame passes `id`, `aria-label` and `data-testid` as attributes. The
 * element it renders is what `useSidebarFrame` reads "wide" from, so give the composable
 * `() => panel.value?.$el`.
 */
import { sidebarPanelClass } from '~/lib/wm/slide'

defineProps<{ open: boolean; wideHidden: boolean }>()
</script>

<template>
  <nav :class="sidebarPanelClass(open, wideHidden)">
    <slot />
  </nav>
</template>
