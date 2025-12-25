<template>
  <UTooltip
    :text="buttonProps?.tooltip"
    class="w-full"
  >
    <UButton
      class="pointer-events-auto w-full justify-center"
      v-bind="{
        ...buttonProps,
        ...$attrs,
      }"
      @click="$emit('click', $event)"
    >
      <template
        v-for="(_, slotName) in $slots"
        #[slotName]="slotProps"
      >
        <slot
          :name="slotName"
          v-bind="slotProps"
        />
      </template>
    </UButton>
  </UTooltip>
</template>

<script setup lang="ts">
import type { ButtonProps } from '@nuxt/ui'

interface IButtonProps extends /* @vue-ignore */ ButtonProps {
  tooltip?: string
}
const buttonProps = defineProps<IButtonProps>()
defineEmits<{ click: [Event] }>()
</script>
