<template>
  <div class="space-y-1.5">
    <div class="relative w-full">
      <UInputTags
        v-model="modelValue"
        :placeholder="shouldFloat ? t('placeholder') : ' '"
        icon="i-lucide-tag"
        size="md"
        color="neutral"
        add-on-blur
        add-on-paste
        :ui="{ root: 'w-full min-h-9 items-center', input: 'py-1 flex-1' }"
        @focus="isFocused = true"
        @blur="isFocused = false"
      />
      <label
        v-if="label"
        :class="[
          'absolute pointer-events-none px-1.5 transition-all',
          shouldFloat
            ? 'text-highlighted text-xs font-medium -top-2.5 left-0'
            : 'text-dimmed text-sm top-1.5 left-7',
        ]"
      >
        <span class="inline-flex bg-default px-1">
          {{ label }}<span
            v-if="required"
            class="text-error"
          > *</span>
        </span>
      </label>
    </div>
    <p
      v-if="suggestions.length"
      class="text-xs text-muted"
    >
      {{ t('existing') }}
      <button
        v-for="name in suggestions"
        :key="name"
        type="button"
        class="ml-1 inline-flex items-center rounded-full bg-elevated/80 px-2 py-0.5 hover:bg-primary/20 transition-colors"
        @click="addSuggestion(name)"
      >
        {{ name }}
      </button>
    </p>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  label?: string
  required?: boolean
}>()

const modelValue = defineModel<string[]>({ required: true })
const { t } = useI18n()

const tagsStore = usePasswordsTagsStore()
const { tags } = storeToRefs(tagsStore)

const isFocused = ref(false)

// Mirror UiInput's floating-label behaviour: label sits inline while the
// field is empty and unfocused, then floats above the border otherwise.
const shouldFloat = computed(
  () => isFocused.value || modelValue.value.length > 0,
)

const suggestions = computed(() => {
  const current = new Set(modelValue.value.map((n) => n.toLowerCase()))
  return tags.value
    .map((tag) => tag.name)
    .filter((name) => !current.has(name.toLowerCase()))
    .slice(0, 20)
})

const addSuggestion = (name: string) => {
  if (modelValue.value.some((n) => n.toLowerCase() === name.toLowerCase())) {
    return
  }
  modelValue.value = [...modelValue.value, name]
}
</script>

<i18n lang="yaml">
de:
  placeholder: Tag tippen und Enter drücken…
  existing: "Vorhandene:"
en:
  placeholder: Type a tag and press Enter…
  existing: "Existing:"
</i18n>
