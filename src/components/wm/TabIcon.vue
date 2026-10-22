<script setup lang="ts">
/**
 * The icon of a tab or window entry: the app's own image (an extension's icon,
 * a system app's coloured icon), otherwise its Iconify icon
 * (`wm.tabDisplayInfo`). Sized by the caller's class.
 */
defineProps<{
  icon: string | undefined
  iconUrl: string | undefined
}>()
</script>

<template>
  <img v-if="iconUrl" :src="iconUrl" alt="" class="shrink-0 object-contain" />
  <Icon v-else-if="icon" :name="icon" class="shrink-0" :aria-hidden="true" />
</template>
