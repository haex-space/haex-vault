<script setup lang="ts">
/**
 * The tab Extra of the editor (spec 036, FR-001, FR-001a, FR-004): the custom fields, the autofill
 * aliases and the attachments.
 * An attachment is saved at once and does not touch the draft or the update token; the parent only
 * refreshes its list (`changed`). A new entry has no attachments yet.
 */
import type { ItemDetail } from '@bindings/ItemDetail'
import type { Draft } from '~/lib/passwords/draft'

defineProps<{
  itemId: string | null
  detail: ItemDetail | null
}>()
const draft = defineModel<Draft>({ required: true })
const emit = defineEmits<{ changed: [] }>()

const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col gap-5" data-testid="entry-editor-extra">
    <SettingsGroup :label="t('passwords.fields.custom')">
      <li class="px-4 py-3" data-no-swipe>
        <PasswordsKeyValues v-model="draft.keyValues" :item-id="itemId" />
      </li>
    </SettingsGroup>

    <PasswordsAutofillAliases
      v-model="draft.aliases"
      :field-keys="draft.keyValues.map((field) => field.key)"
    />

    <PasswordsAttachments
      v-if="itemId !== null && detail"
      :item-id="itemId"
      :attachments="detail.attachments"
      @changed="emit('changed')"
    />
  </div>
</template>
