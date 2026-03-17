<template>
  <div
    @focusin="isFocused = true"
    @focusout="isFocused = false"
  >
    <UTextarea
      :id
      v-model="value"
      :placeholder="effectivePlaceholder"
      :readonly="readOnly"
      :size
      class="w-full"
      :ui="{ base: 'peer', root: 'group' }"
      :data-size="size || 'md'"
      v-bind="filteredAttrs"
    >
      <label
        v-if="label"
        :class="[
          'floating-label absolute pointer-events-none px-1.5 transition-all text-highlighted text-xs font-medium -top-2.5 left-0',
          'group-has-placeholder-shown:text-sm group-has-placeholder-shown:text-dimmed group-has-placeholder-shown:font-normal',
          'group-has-focus:text-highlighted group-has-focus:text-xs group-has-focus:font-medium group-has-focus:-top-2.5 group-has-focus:left-0',
          labelTopClass,
        ]"
      >
        <span class="inline-flex bg-default px-1">
          {{ label }}
        </span>
      </label>

      <template #trailing>
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
    </UTextarea>
  </div>
</template>

<script setup lang="ts">
import type { TextareaProps } from '@nuxt/ui'

interface ITextareaProps extends /* @vue-ignore */ TextareaProps {
  tooltip?: string
  withCopyButton?: boolean
  readOnly?: boolean
  label?: string
}

const props = defineProps<ITextareaProps>()

const { size, readOnly, label } = toRefs(props)

const attrs = useAttrs()
const placeholder = computed(() => attrs.placeholder as string | undefined)
const filteredAttrs = computed(() => {
  const { placeholder: _, ...rest } = attrs
  return rest
})

const hasDistinctPlaceholder = computed(() => !!label?.value && !!placeholder.value && placeholder.value !== ' ')

const isFocused = ref(false)
const effectivePlaceholder = computed(() => {
  if (hasDistinctPlaceholder.value && !isFocused.value) return ' '
  return placeholder.value || ' '
})

const id = useId()
const value = defineModel<string | undefined>()
const { copy, copied } = useClipboard()
const { t } = useI18n()

const labelTopClass = computed(() => {
  const topPositions: Record<string, string> = {
    xs: 'group-has-placeholder-shown:top-0.5',
    sm: 'group-has-placeholder-shown:top-1',
    md: 'group-has-placeholder-shown:top-1.5',
    lg: 'group-has-placeholder-shown:top-2',
    xl: 'group-has-placeholder-shown:top-2.5',
  }

  return topPositions[props.size || 'md'] || 'group-has-placeholder-shown:top-1.5'
})
</script>

<i18n lang="yaml">
de:
  copy: Kopieren
en:
  copy: Copy
</i18n>
