<template>
  <UAccordion v-if="items?.length">
    <template #title>
      <h3>{{ title }}</h3>
    </template>

    <ul class="space-y-0.5">
      <li
        v-for="(item, index) in items"
        :key="index"
        class="flex items-center justify-between px-4 py-0.5"
      >
        <div class="flex items-center gap-1">
          <input
            :id="Object.keys(item).at(0)"
            type="checkbox"
            class="checkbox"
            :checked="Object.values(item).at(0)"
          >
          <label
            class="label-text text-base"
            :for="Object.keys(item).at(0)"
          >{{ Object.keys(item).at(0) }}</label>
        </div>
      </li>
    </ul>
  </UAccordion>
</template>

<script setup lang="ts">
defineProps<{
  title: string
  items?: Record<string, boolean>[]
}>()
</script>
