<script setup lang="ts">
/**
 * The fields that belong to one row of the rights list and only make sense while the row is on:
 * the entries of the password manager, the exceptions of the browser. An inset block with an
 * accent line under its row, so it is clear where it belongs. A row of a `SettingsGroup`.
 */
</script>

<template>
  <li
    class="flex flex-col gap-3 border-l-4 border-primary/60 bg-background/60 py-3 pr-4 pl-4"
  >
    <slot />
  </li>
</template>
