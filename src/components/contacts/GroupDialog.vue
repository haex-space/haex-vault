<script setup lang="ts">
/**
 * Create or rename a group of the address book (spec 051, FR-003). A group keeps a name; the
 * backend refuses an empty one and the dialog says so.
 */
import { toast } from 'vue-sonner'
import type { ContactGroup } from '@bindings/ContactGroup'

const props = defineProps<{
  /** The group to rename, or `null` to create one. */
  group: ContactGroup | null
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { errString } = useErrorString()
const store = useContactsStore()
const { groupSaveAsync } = useContacts()

const name = ref('')
const error = ref<string | null>(null)
const saving = ref(false)

watch(open, (isOpen) => {
  if (!isOpen) return
  name.value = props.group?.name ?? ''
  error.value = null
})

async function saveAsync() {
  if (!name.value.trim()) {
    error.value = t('contacts.groups.nameRequired')
    return
  }
  saving.value = true
  try {
    await groupSaveAsync(name.value, props.group?.id)
    await store.quietReloadAsync()
    open.value = false
  } catch (cause) {
    error.value = errString(cause)
    toast.error(error.value)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="group ? t('contacts.groups.rename') : t('contacts.groups.new')"
  >
    <template #content>
      <form
        class="flex flex-col gap-4"
        data-testid="contacts-group-form"
        @submit.prevent="saveAsync"
      >
        <UiInput
          id="contacts-group-name"
          v-model="name"
          :label="t('contacts.groups.name')"
          :labels="fieldLabels.input.value"
          data-testid="contacts-group-name"
        />
        <p v-if="error" class="text-sm text-destructive" role="alert">
          {{ error }}
        </p>
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="open = false">{{
            t('contacts.cancel')
          }}</UiButton>
          <UiButton
            type="submit"
            :loading="saving"
            data-testid="contacts-group-save"
            >{{ t('contacts.save') }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
</template>
