<script setup lang="ts">
/**
 * The references of one editor field (spec 036, US7, research R11), shown below the field: its
 * marks as the backend reads them from the text (asked 200 ms after the last keystroke), each with a
 * button that takes that placeholder out of the text. The input stays a plain text field; the
 * window never parses a placeholder itself. "Verweis einfügen" sits in the field
 * (`ReferenceInsert.vue`).
 */
import type { RefMark } from '@bindings/RefMark'

const props = defineProps<{
  text: string
  /** Part of the test ids. */
  kind: string
}>()

const emit = defineEmits<{
  'update:text': [value: string]
}>()

const { t } = useI18n()
const { referencesParseAsync } = usePasswords()

const parsed = ref<RefMark[]>([])
/** The text `parsed` belongs to; its offsets fit no other text. */
const parsedFor = ref<string | null>(null)
let request = 0

async function parseAsync(text: string) {
  const mine = ++request
  if (!text.includes('{$')) {
    parsed.value = []
    parsedFor.value = text
    return
  }
  try {
    const marks = await referencesParseAsync(text)
    if (mine !== request) return
    parsed.value = marks
    parsedFor.value = text
  } catch {
    if (mine !== request) return
    parsed.value = []
    parsedFor.value = text
  }
}

watchDebounced(() => props.text, parseAsync, { debounce: 200 })
onMounted(() => void parseAsync(props.text))

/** Removing waits for the parse of the current text (200 ms debounce), or offsets cut wrongly. */
const canRemove = computed(() => parsedFor.value === props.text)

/** UTF-16 offsets, as the backend reports them. */
function removeMark(mark: RefMark) {
  const text = props.text
  if (parsedFor.value !== text) return
  if (!text.startsWith('{$', mark.start)) return
  emit('update:text', text.slice(0, mark.start) + text.slice(mark.end))
}
</script>

<template>
  <div
    v-if="parsed.length"
    class="flex flex-wrap items-center gap-1.5"
    :data-testid="`passwords-reference-field-${kind}`"
  >
    <template
      v-for="mark in parsed"
      :key="`${mark.start}-${mark.sourceItemId}`"
    >
      <span class="inline-flex items-center gap-0.5">
        <PasswordsReferenceValue
          :marks="[mark]"
          :text="null"
          :kind="kind"
          :navigable="false"
        />
        <UiButtonClose
          :label="t('passwords.references.remove')"
          size="icon-sm"
          :disabled="!canRemove"
          class="size-6"
          :data-testid="`passwords-reference-remove-${kind}`"
          icon-class="size-3"
          @click="removeMark(mark)"
        />
      </span>
    </template>
  </div>
</template>
