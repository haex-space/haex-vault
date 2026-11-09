<script setup lang="ts">
/**
 * Create or edit a folder (spec 034, US2, FR-009): name, description, icon and color. A folder
 * keeps a name; the backend refuses an empty one and the dialog says so.
 */
import { toast } from 'vue-sonner'
import type { GroupRow } from '@bindings/GroupRow'
import { ENTRY_COLORS, ENTRY_ICONS } from '~/lib/passwords/icons'
import { clearOnReselect } from '~/lib/ui/radio'

const props = defineProps<{
  /** The folder to edit, or `null` to create one. */
  group: GroupRow | null
  /** The folder a new one goes into; `null` is the top level. */
  parentId: string | null
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { errString } = useErrorString()
const store = usePasswordsStore()
const { createGroupAsync, updateGroupAsync } = usePasswords()

const name = ref('')
const description = ref('')
const icon = ref<string | null>(null)
const color = ref<string | null>(null)
/** Whether a save was tried without a name; a failed save goes out as a toast. */
const nameMissing = ref(false)
const saving = ref(false)

watch(open, (isOpen) => {
  if (!isOpen) return
  name.value = props.group?.name ?? ''
  description.value = props.group?.description ?? ''
  icon.value = props.group?.icon ?? null
  color.value = props.group?.color ?? null
  nameMissing.value = false
})
// The hint under the name goes away once one is typed.
watch(name, (value) => {
  if (nameMissing.value && value.trim()) nameMissing.value = false
})

async function saveAsync() {
  if (!name.value.trim()) {
    nameMissing.value = true
    return
  }
  nameMissing.value = false
  saving.value = true
  try {
    if (props.group) {
      await updateGroupAsync(props.group.id, {
        name: name.value,
        description: description.value || null,
        icon: icon.value,
        color: color.value,
      })
    } else {
      await createGroupAsync({
        name: name.value,
        description: description.value || undefined,
        icon: icon.value ?? undefined,
        color: color.value ?? undefined,
        parentId: props.parentId ?? undefined,
      })
    }
    await store.quietReloadAsync()
    open.value = false
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="group ? t('passwords.folders.edit') : t('passwords.folders.new')"
  >
    <template #content>
      <form
        class="flex flex-col gap-4"
        data-testid="passwords-folder-form"
        @submit.prevent="saveAsync"
      >
        <UiInput
          id="pw-folder-name"
          v-model="name"
          :label="t('passwords.fields.name')"
          :labels="fieldLabels.input.value"
          data-testid="passwords-folder-name"
        />
        <p v-if="nameMissing" class="text-sm text-destructive" role="alert">
          {{ t('passwords.folders.nameRequired') }}
        </p>
        <UiInput
          id="pw-folder-description"
          v-model="description"
          :label="t('passwords.fields.description')"
          :labels="fieldLabels.input.value"
        />
        <ShadcnRadioGroup
          v-model="icon"
          class="flex flex-wrap gap-1.5"
          :aria-label="t('passwords.fields.icon')"
        >
          <UiRadioGroupTile
            v-for="candidate in ENTRY_ICONS"
            :key="candidate"
            :value="candidate"
            :aria-label="candidate.replace('lucide:', '')"
            class="flex size-9 items-center justify-center rounded-lg border border-transparent bg-muted hover:bg-accent data-[state=checked]:border-primary data-[state=checked]:bg-primary/10"
            @select="
              clearOnReselect($event, icon === candidate, () => (icon = null))
            "
          >
            <Icon :name="candidate" class="size-5" />
          </UiRadioGroupTile>
        </ShadcnRadioGroup>
        <ShadcnRadioGroup
          v-model="color"
          class="flex flex-wrap gap-1.5"
          :aria-label="t('passwords.fields.color')"
        >
          <UiRadioGroupTile
            v-for="candidate in ENTRY_COLORS"
            :key="candidate"
            :value="candidate"
            :aria-label="candidate"
            class="size-7 rounded-full border-2 border-transparent data-[state=checked]:border-foreground"
            :style="{ backgroundColor: candidate }"
            @select="
              clearOnReselect($event, color === candidate, () => (color = null))
            "
          />
        </ShadcnRadioGroup>
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="open = false">{{
            t('passwords.cancel')
          }}</UiButton>
          <UiButton
            type="submit"
            :loading="saving"
            data-testid="passwords-folder-save"
            >{{ t('passwords.save') }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
</template>
