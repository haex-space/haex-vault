<template>
  <UInputTags
    v-model="modelValue"
    :placeholder="t('placeholder')"
    icon="i-lucide-tag"
    size="md"
    color="neutral"
    add-on-blur
    add-on-paste
  />
  <p
    v-if="suggestions.length"
    class="mt-1.5 text-xs text-muted"
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
</template>

<script setup lang="ts">
const modelValue = defineModel<string[]>({ required: true })
const { t } = useI18n()

const tagsStore = usePasswordsTagsStore()
const { tags } = storeToRefs(tagsStore)

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
