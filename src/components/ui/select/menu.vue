<template>
  <div class="relative">
    <label
      v-if="label"
      class="absolute pointer-events-none px-1.5 z-10 text-xs font-medium -top-1.5 left-1 text-highlighted"
    >
      <span class="inline-flex bg-default px-1">
        {{ label }}
      </span>
    </label>

    <USelectMenu
      v-bind="$attrs"
      :items="items"
      :placeholder="placeholder"
    >
      <template
        v-for="(_, slotName) in $slots"
        #[slotName]="slotProps"
      >
        <slot
          :name="slotName"
          v-bind="slotProps || {}"
        />
      </template>
    </USelectMenu>
  </div>
</template>

<script setup lang="ts">
defineOptions({ inheritAttrs: false })

defineProps<{
  label?: string
  items?: any[]
  placeholder?: string
}>()
</script>
