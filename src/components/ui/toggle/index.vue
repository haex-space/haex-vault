<template>
  <label class="inline-flex items-center cursor-pointer">
    <input
      type="checkbox"
      :checked="modelValue"
      class="sr-only peer"
      @change="
        $emit('update:modelValue', ($event.target as HTMLInputElement).checked)
      "
    />
    <div
      class="relative w-11 h-6 bg-accented border peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/30 rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full after:content-[''] after:absolute after:bg after:border-2 after:shadow-md after:rounded-full after:h-[22px] after:w-[22px] after:transition-all peer-checked:bg-primary peer-checked:after:bg-white peer-checked:after:border-2 peer-checked:after:border-primary/40 peer-checked:after:shadow-lg peer-checked:border-primary"
    />
  </label>
</template>

<script setup lang="ts">
defineProps<{
  modelValue: boolean
}>()

defineEmits<{
  'update:modelValue': [value: boolean]
}>()
</script>
