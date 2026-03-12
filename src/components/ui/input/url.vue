<template>
  <UiInput
    v-model.trim="value"
    :autofocus
    :check-input="checkInput"
    :label="label || t('url')"
    :placeholder="placeholder || t('url')"
    :read-only="readOnly"
    :rules
    :with-copy-button
    @keyup="(e) => $emit('keyup', e)"
  >
    <template #trailing>
      <UiButton
        color="neutral"
        variant="link"
        size="sm"
        icon="streamline:web"
        :disabled="!value?.length"
        :tooltip="t('browse')"
        @click="openUrl(`${value}`)"
      />
    </template>
  </UiInput>
</template>

<script setup lang="ts">
import type { ZodSchema } from 'zod'
import { openUrl } from '@tauri-apps/plugin-opener'

const { t } = useI18n()

const value = defineModel<string | null | undefined>()

defineProps({
  label: { type: String, default: undefined },
  placeholder: { type: String, default: undefined },
  checkInput: Boolean,
  rules: { type: Object as PropType<ZodSchema>, default: undefined },
  autofocus: Boolean,
  withCopyButton: Boolean,
  readOnly: Boolean,
})

defineEmits<{
  keyup: [KeyboardEvent]
}>()
</script>

<i18n lang="yaml">
de:
  url: Url
  browse: Url öffnen

en:
  url: Url
  browse: Open url
</i18n>
