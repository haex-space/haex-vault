<script setup lang="ts">
/**
 * The `⋯` button of a window header (spec 090-app-header-bar, FR-014, research R10): it opens and
 * closes the window's side panel (`WmHeader` teleports the controls that did not fit into it, each
 * in its menu form). The panel is the window's, like the sidebar of an app on the other side.
 */
defineProps<{ open: boolean }>()
defineEmits<{ toggle: [] }>()

const { t } = useI18n()
</script>

<template>
  <UiButton
    variant="ghost"
    size="icon"
    class="shrink-0"
    :aria-label="t('wm.header.overflow')"
    :aria-expanded="open"
    :tooltip="t('wm.header.overflow')"
    data-testid="wm-header-overflow"
    @pointerdown.stop
    @click="$emit('toggle')"
  >
    <Icon
      name="lucide:ellipsis"
      class="size-4 text-primary"
      :aria-hidden="true"
    />
  </UiButton>
</template>
