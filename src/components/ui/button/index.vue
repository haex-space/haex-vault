<template>
  <span class="inline-flex">
    <UTooltip :text="buttonProps?.tooltip">
      <UButton
        class="pointer-events-auto"
        v-bind="{
          ...buttonProps,
          ...$attrs,
        }"
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

// Vue 3.5.34's SFC compiler cannot resolve type imports from @nuxt/ui's
// virtual component bundle — `@vue-ignore` keeps the inherited props as
// fallthrough attrs at runtime, matching the 3.2 behaviour described in
// the compiler warning.
interface IButtonProps extends /* @vue-ignore */ ButtonProps {
  tooltip?: string
}
const buttonProps = defineProps<IButtonProps>()

defineOptions({ inheritAttrs: false })
</script>
