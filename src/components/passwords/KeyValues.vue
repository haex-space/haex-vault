<script setup lang="ts">
/**
 * The custom fields of the entry editor (spec 034, FR-002): name and value per field, add and
 * remove. The values are plain, visible fields holding the stored values with their placeholders.
 */
import type { KeyValueDraft } from '~/lib/passwords/draft'

defineProps<{
  /** The entry being edited, for the reference picker (spec 036); `null` for a new one. */
  itemId?: string | null
}>()

const fields = defineModel<KeyValueDraft[]>({ required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { copyText } = usePasswordsCopy()

function add() {
  fields.value = [...fields.value, { id: null, key: '', value: '' }]
}

function remove(index: number) {
  fields.value = fields.value.filter((_, i) => i !== index)
}

function setKey(index: number, key: string) {
  fields.value = fields.value.map((field, i) =>
    i === index ? { ...field, key } : field,
  )
}

function setValue(index: number, value: string) {
  fields.value = fields.value.map((field, i) =>
    i === index ? { ...field, value } : field,
  )
}
</script>

<template>
  <div class="flex flex-col gap-2" data-testid="passwords-keyvalues">
    <div
      v-for="(field, index) in fields"
      :key="field.id ?? `new-${index}`"
      class="flex flex-wrap items-center gap-2"
    >
      <div class="min-w-32 flex-1">
        <UiInput
          :model-value="field.key"
          :placeholder="t('passwords.editor.fieldName')"
          :aria-label="t('passwords.editor.fieldName')"
          :data-testid="`passwords-kv-key-${index}`"
          @update:model-value="setKey(index, String($event ?? ''))"
        />
      </div>
      <div class="min-w-32 flex-1">
        <UiInput
          :model-value="field.value"
          :labels="fieldLabels.input.value"
          autocomplete="off"
          :placeholder="t('passwords.editor.fieldValue')"
          :aria-label="t('passwords.editor.fieldValue')"
          :data-testid="`passwords-kv-value-${index}`"
          @update:model-value="setValue(index, String($event ?? ''))"
        >
          <template #append>
            <PasswordsCopyButton
              v-if="field.value"
              :label="field.key"
              @copy="
                copyText(field.value, field.key, {
                  itemId: itemId ?? null,
                  field: { kind: 'keyValue', key: field.key },
                })
              "
            />
            <PasswordsReferenceInsert
              :text="field.value"
              :item-id="itemId ?? null"
              :kind="`kv-${index}`"
              @update:text="setValue(index, $event)"
            />
          </template>
        </UiInput>
      </div>
      <UiButtonDelete
        :label="t('passwords.editor.removeField')"
        :data-testid="`passwords-kv-remove-${index}`"
        @click="remove(index)"
      />
      <PasswordsReferenceField
        class="basis-full"
        :text="field.value"
        :kind="`kv-${index}`"
        @update:text="setValue(index, $event)"
      />
    </div>
    <UiButton
      type="button"
      variant="outline"
      size="sm"
      class="self-start"
      data-testid="passwords-kv-add"
      @click="add"
    >
      <Icon name="lucide:plus" class="size-4" />
      {{ t('passwords.editor.addField') }}
    </UiButton>
  </div>
</template>
