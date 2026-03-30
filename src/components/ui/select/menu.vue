<template>
  <div
    class="relative group"
    @focusin="isFocused = true"
    @focusout="isFocused = false"
  >
    <label
      v-if="label"
      :class="[
        'absolute pointer-events-none px-1.5 transition-all z-10',
        isLabelFloating
          ? 'text-highlighted text-xs font-medium -top-2.5 left-0'
          : 'text-sm text-dimmed font-normal top-2 left-2',
      ]"
    >
      <span class="inline-flex bg-default px-1">
        {{ label }}
      </span>
    </label>

    <USelectMenu
      v-bind="$attrs"
      :items="items"
      :placeholder="isLabelFloating ? placeholder : ' '"
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

const isFocused = ref(false)

const hasValue = computed(() => {
  const attrs = useAttrs()
  const modelValue = attrs.modelValue ?? attrs['model-value']
  if (modelValue === undefined || modelValue === null || modelValue === '') return false
  if (Array.isArray(modelValue)) return modelValue.length > 0
  return true
})

const isLabelFloating = computed(() => isFocused.value || hasValue.value)
</script>
