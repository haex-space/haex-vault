<template>
  <div>
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
      v-bind="$attrs"
    >
      <label
        v-if="label"
        :class="[
          'floating-label absolute pointer-events-none px-1.5 transition-all text-highlighted text-xs font-medium -top-2.5 left-0',
          'group-has-placeholder-shown:text-sm group-has-placeholder-shown:text-dimmed group-has-placeholder-shown:font-normal',
          'group-has-focus:text-highlighted group-has-focus:text-xs group-has-focus:font-medium group-has-focus:-top-2.5 group-has-focus:left-0',
          errors?.length ? 'text-red-500' : '',
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
    <div
      v-if="errors?.length"
      class="mt-1.5 space-y-1"
    >
      <p
        v-for="errorMsg in errors"
        :key="errorMsg"
        class="text-xs text-red-500 dark:text-red-400"
      >
        {{ errorMsg }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { InputProps } from '@nuxt/ui'
import type { AcceptableValue } from '@nuxt/ui/runtime/types/utils.js'
import type { ZodSchema } from 'zod'

const value = defineModel<AcceptableValue | undefined>()
const errors = defineModel<string[]>('errors', { default: () => [] })

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
    schema?: ZodSchema
    check?: boolean
    customValidators?: Array<(value: AcceptableValue | undefined) => string | null>
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

// Validation function
const validate = () => {
  const validationErrors: string[] = []

  // Run schema validation
  if (props.schema) {
    const result = props.schema.safeParse(value.value)
    if (!result.success) {
      validationErrors.push(...result.error.errors.map((err) => err.message))
    }
  }

  // Run custom validators
  if (props.customValidators) {
    for (const validator of props.customValidators) {
      const error = validator(value.value)
      if (error) {
        validationErrors.push(error)
      }
    }
  }

  errors.value = validationErrors
  return validationErrors.length === 0
}

// Watch for value changes and validate
watch(value, () => {
  if ((props.schema || props.customValidators) && (props.check || errors.value.length > 0)) {
    validate()
  }
})

// Watch for check prop changes to trigger validation
watch(() => props.check, (newCheck) => {
  if (newCheck && (props.schema || props.customValidators)) {
    validate()
  }
})
</script>

<i18n lang="yaml">
de:
  copy: Kopieren

en:
  copy: Copy
</i18n>
