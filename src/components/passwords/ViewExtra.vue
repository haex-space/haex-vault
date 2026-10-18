<script setup lang="ts">
/**
 * The tab Extra of an entry in view mode (spec 036, FR-001, FR-005): the custom fields, the
 * attachments and the passkeys. The custom values show as stored, unmasked (amends spec 034
 * FR-005); a value with references shows its marks and a masked resolved value, like the user
 * name. Only the value itself carries `data-no-swipe`, so it can be selected without changing the
 * tab (FR-003).
 */
import type { ItemDetail } from '@bindings/ItemDetail'

const emit = defineEmits<{
  changed: []
}>()

const props = defineProps<{
  itemId: string
  detail: ItemDetail
  readonly?: boolean
}>()

const { t } = useI18n()
const { revealAsync } = usePasswords()
const { copyField } = usePasswordsCopy()

/** The marks of a custom field that holds references (spec 036). */
function marksOf(id: string) {
  return (
    props.detail.references.keyValues.find((entry) => entry.id === id)?.marks ??
    []
  )
}
</script>

<template>
  <div class="flex flex-col gap-4" data-testid="entry-view-extra">
    <SettingsGroup
      v-if="detail.keyValues.length"
      :label="t('passwords.fields.custom')"
    >
      <SettingsRow
        v-for="field in detail.keyValues"
        :key="field.id"
        :title="field.key ?? ''"
      >
        <template v-if="marksOf(field.id).length" #below>
          <PasswordsReferenceValue
            :marks="marksOf(field.id)"
            :text="field.value"
            kind="keyValue"
          />
        </template>
        <PasswordsMaskedValue
          v-if="marksOf(field.id).length"
          :fetch="
            async () =>
              (
                await revealAsync(itemId, {
                  kind: 'keyValue',
                  id: field.id,
                })
              ).value
          "
          :identity="`${itemId}:${field.id}:${field.value}`"
          kind="keyValue"
          present
          :label="field.key ?? ''"
        />
        <span
          v-else
          class="min-w-0 truncate font-mono text-sm select-text"
          :class="field.hasValue ? '' : 'text-muted-foreground'"
          data-no-swipe
          data-testid="passwords-value-keyValue"
          >{{ field.hasValue ? field.value : t('passwords.noValue') }}</span
        >
        <PasswordsCopyButton
          v-if="field.hasValue"
          :label="field.key ?? ''"
          @copy="
            copyField(
              itemId,
              { kind: 'keyValue', id: field.id },
              field.key ?? '',
            )
          "
        />
      </SettingsRow>
    </SettingsGroup>

    <PasswordsAttachments
      :item-id="itemId"
      :attachments="detail.attachments"
      :readonly="readonly"
      @changed="emit('changed')"
    />

    <PasswordsPasskeys
      :item-id="itemId"
      :passkeys="detail.passkeys"
      :readonly="readonly"
      @changed="emit('changed')"
    />
  </div>
</template>
