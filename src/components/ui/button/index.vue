<template>
  <span class="inline-flex">
    <UTooltip :text="buttonProps?.tooltip">
      <UButton
        class="pointer-events-auto"
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
  </span>
</template>

<script setup lang="ts">
import type { ButtonProps } from '@nuxt/ui'

interface IButtonProps extends /* @vue-ignore */ ButtonProps {
  tooltip?: string
}
const buttonProps = defineProps<IButtonProps>()
defineEmits<{ click: [Event] }>()
</script>
