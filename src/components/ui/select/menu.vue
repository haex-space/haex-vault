<template>
  <div
    class="relative w-full"
  >
    <label
      v-if="label"
      class="absolute pointer-events-none px-1.5 z-10 text-xs font-medium left-0 text-highlighted" style="top: -9px"
    >
      <span class="inline-flex bg-default px-1">
        {{ label }}
      </span>
    </label>

    <USelectMenu
      v-bind="filteredAttrs"
      :items="items"
      :placeholder="placeholder"
      class="w-full"
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

const filteredAttrs = computed(() => {
  const { class: _, ...rest } = useAttrs()
  return rest
})
</script>
