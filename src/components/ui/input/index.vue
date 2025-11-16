<template>
  <UInput
    v-model="value"
    :placeholder="placeholder || ' '"
    :disabled="readOnly"
    :leading-icon
    :size
    :type
    :icon
    :ui="{ base: 'peer', root: 'group' }"
    :data-size="size || 'md'"
    @change="(e) => $emit('change', e)"
    @blur="(e) => $emit('blur', e)"
    @keyup="(e: KeyboardEvent) => $emit('keyup', e)"
    @keydown="(e: KeyboardEvent) => $emit('keydown', e)"
  >
    <label
      v-if="label"
      :class="[
        'floating-label absolute pointer-events-none px-1.5 transition-all text-highlighted text-xs font-medium -top-2.5 left-0',
        'group-has-placeholder-shown:text-sm group-has-placeholder-shown:text-dimmed group-has-placeholder-shown:font-normal',
        'group-has-focus:text-highlighted group-has-focus:text-xs group-has-focus:font-medium group-has-focus:-top-2.5 group-has-focus:left-0',
        labelLeftClass,
        labelTopClass,
      ]"
    >
      <span class="inline-flex bg-default px-1">
        {{ label }}
      </span>
    </label>

    <template #trailing>
      <slot name="trailing" />

      <UiButton
        v-show="withCopyButton"
        :color="copied ? 'success' : 'neutral'"
        :tooltip="t('copy')"
        :icon="copied ? 'mdi:check' : 'mdi:content-copy'"
        size="sm"
        variant="link"
        @click="copy(`${value}`)"
      />
    </template>

    <template
      v-for="(_, slotName) in filteredSlots"
      #[slotName]="slotProps"
    >
      <slot
        :name="slotName"
        v-bind="slotProps"
      />
    </template>
  </UInput>
</template>

<script setup lang="ts">
import type { InputProps } from '@nuxt/ui'
import type { AcceptableValue } from '@nuxt/ui/runtime/types/utils.js'

const value = defineModel<AcceptableValue | undefined>()

interface IInputProps extends /* @vue-ignore */ InputProps {
  tooltip?: string
}

const props = defineProps<
  IInputProps & {
    withCopyButton?: boolean
    readOnly?: boolean
    label?: string
    leadingIcon?: string
    icon?: string
    size?: InputProps['size']
  }
>()

const {
  placeholder,
  size,
  type,
  withCopyButton,
  readOnly,
  label,
  leadingIcon,
} = toRefs(props)

defineEmits<{
  change: [Event]
  blur: [Event]
  keyup: [KeyboardEvent]
  keydown: [KeyboardEvent]
}>()

const { copy, copied } = useClipboard()

const { t } = useI18n()

// Label left position when leading icon is present, based on input size
const labelLeftClass = computed(() => {
  if (!props.leadingIcon && !props.icon) return ''

  const leftPositions: Record<string, string> = {
    xs: 'group-has-placeholder-shown:left-5',
    sm: 'group-has-placeholder-shown:left-6',
    md: 'group-has-placeholder-shown:left-7',
    lg: 'group-has-placeholder-shown:left-8',
    xl: 'group-has-placeholder-shown:left-9',
  }

  return (
    leftPositions[props.size || 'md'] || 'group-has-placeholder-shown:left-7'
  )
})

// Label top position based on input size (for unfocused state with placeholder shown)
const labelTopClass = computed(() => {
  const topPositions: Record<string, string> = {
    xs: 'group-has-placeholder-shown:top-0.5',
    sm: 'group-has-placeholder-shown:top-1',
    md: 'group-has-placeholder-shown:top-1.5',
    lg: 'group-has-placeholder-shown:top-2',
    xl: 'group-has-placeholder-shown:top-2.5',
  }

  return topPositions[props.size || 'md'] || 'group-has-placeholder-shown:top-2'
})

const filteredSlots = computed(() => {
  return Object.fromEntries(
    Object.entries(useSlots()).filter(([name]) => name !== 'trailing'),
  )
})
</script>

<i18n lang="yaml">
de:
  copy: Kopieren

en:
  copy: Copy
</i18n>
