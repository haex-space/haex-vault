<script setup lang="ts">
/**
 * The tag manager (spec 034, US2, FR-011; by selection since spec 100): one dropdown over the tags
 * of the vault and, once one is chosen, its editor — rename, color, delete, merge into another tag
 * and how often it is used in the password manager and in Wallet. Tags are one namespace for both apps, so this dialog
 * opens from either. A name needs a character that is not a space, and a name that exists already
 * (ignoring case and the form of umlauts) is refused. Deleting asks first, and only the user can do
 * any of this (FR-028).
 */
import { toast } from 'vue-sonner'
import type { MergedTag } from '~/lib/passwords/remote'
import { tagUsageOf } from '~/lib/passwords/scope'

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { errString } = useErrorString()
const store = usePasswordsStore()
const { renameTagAsync, setTagColorAsync, deleteTagAsync } = usePasswords()

const selectedId = ref<string | null>(null)
const editing = ref(false)
const editSession = ref(0)
const name = ref('')
const error = ref<string | null>(null)
const deleting = ref<MergedTag | null>(null)
// The tag a merge started from stays here while its dialog is open: the merge removes it from the
// list, and the dialog must outlive that.
const mergeSource = ref<MergedTag | null>(null)
const mergeOpen = ref(false)

function startMerge(tag: MergedTag) {
  mergeSource.value = tag
  mergeOpen.value = true
}

const selected = computed(
  () => store.displayTags.find((tag) => tag.id === selectedId.value) ?? null,
)
const usage = computed(() =>
  selected.value ? tagUsageOf(selected.value.ids, store.headers) : null,
)

// A tag that is gone (deleted or merged here or on another device) leaves no selection behind.
watch(
  () => store.displayTags,
  (tags) => {
    if (selectedId.value && !tags.some((tag) => tag.id === selectedId.value))
      selectedId.value = null
  },
)
// Another tag starts with its own, unedited name.
watch(selectedId, () => {
  ++editSession.value
  editing.value = false
  error.value = null
})
// The hint refers to the name that was refused: it goes away once the name is changed.
watch(name, () => {
  error.value = null
})

function start(tag: MergedTag) {
  ++editSession.value
  editing.value = true
  name.value = tag.name
  error.value = null
}

async function renameAsync(tag: MergedTag) {
  const session = editSession.value
  try {
    await renameTagAsync(tag.ids, name.value)
    if (session === editSession.value) {
      editing.value = false
      error.value = null
    }
    await store.quietReloadAsync()
  } catch (cause) {
    // A refused name belongs to the name field; any other failure is a toast.
    const reason = (cause as { reason?: string }).reason
    if (reason === 'exists' || reason === 'empty' || reason === 'too_long') {
      if (session === editSession.value)
        error.value = t(`passwords.tags.errors.${reason}`)
    } else toast.error(errString(cause))
  }
}

async function colorAsync(tag: MergedTag, color: string | null) {
  try {
    await setTagColorAsync(tag.ids, color)
    await store.quietReloadAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function confirmDeleteAsync() {
  const tag = deleting.value
  deleting.value = null
  if (!tag) return
  try {
    await deleteTagAsync(tag.ids)
    selectedId.value = null
    await store.quietReloadAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('passwords.tags.manage')">
    <template #content>
      <p
        v-if="store.displayTags.length === 0"
        class="py-4 text-sm text-muted-foreground"
      >
        {{ t('passwords.tags.none') }}
      </p>
      <div v-else class="flex flex-col gap-4">
        <PasswordsTagPicker
          v-model="selectedId"
          :tags="store.displayTags"
          :aria-label="t('passwords.tags.choose')"
          data-testid="passwords-tag-picker"
        />
        <p v-if="!selected" class="text-sm text-muted-foreground">
          {{ t('passwords.tags.selectOne') }}
        </p>
        <SettingsGroup v-else>
          <li
            :key="selected.id"
            class="px-4 py-2.5"
            :data-testid="`passwords-tag-${selected.id}`"
          >
            <div class="flex flex-wrap items-center gap-2">
              <template v-if="editing">
                <div class="min-w-32 flex-1">
                  <UiInput
                    v-model="name"
                    :aria-label="t('passwords.tags.name')"
                    :data-testid="`passwords-tag-name-${selected.id}`"
                    @keydown.enter.prevent="renameAsync(selected)"
                    @keydown.esc.prevent="editing = false"
                  />
                  <p
                    v-if="error"
                    class="mt-1 text-sm text-destructive"
                    role="alert"
                  >
                    {{ error }}
                  </p>
                </div>
                <UiButton
                  size="sm"
                  :data-testid="`passwords-tag-save-${selected.id}`"
                  @click="renameAsync(selected)"
                  >{{ t('passwords.save') }}</UiButton
                >
              </template>
              <template v-else>
                <span
                  class="min-w-0 flex-1 truncate"
                  :style="
                    selected.color ? { color: selected.color } : undefined
                  "
                  >{{ selected.name }}</span
                >
                <PasswordsTagUsage v-if="usage" :usage="usage" />
                <PasswordsColorSelect
                  :model-value="selected.color"
                  :data-testid="`passwords-tag-color-${selected.id}`"
                  @update:model-value="colorAsync(selected, $event)"
                />
                <UiButton
                  type="button"
                  variant="ghost"
                  size="icon"
                  class="size-8 text-muted-foreground hover:bg-background hover:text-foreground focus-visible:text-foreground"
                  :aria-label="t('passwords.tags.rename')"
                  :tooltip="t('passwords.tags.rename')"
                  :data-testid="`passwords-tag-rename-${selected.id}`"
                  @click="start(selected)"
                >
                  <Icon
                    name="lucide:pencil"
                    class="size-4"
                    aria-hidden="true"
                  />
                </UiButton>
                <UiButton
                  v-if="store.displayTags.length > 1"
                  type="button"
                  variant="ghost"
                  size="icon"
                  class="size-8 text-muted-foreground hover:bg-background hover:text-foreground focus-visible:text-foreground"
                  :aria-label="t('passwords.tags.merge')"
                  :tooltip="t('passwords.tags.merge')"
                  data-testid="passwords-tag-merge"
                  @click="startMerge(selected)"
                >
                  <Icon name="lucide:merge" class="size-4" aria-hidden="true" />
                </UiButton>
                <DeleteButton
                  :label="t('passwords.tags.delete')"
                  :data-testid="`passwords-tag-delete-${selected.id}`"
                  @click="deleting = selected"
                />
              </template>
            </div>
          </li>
        </SettingsGroup>
      </div>
    </template>
  </UiDrawerModal>

  <PasswordsTagMergeDialog
    v-if="mergeSource"
    v-model:open="mergeOpen"
    :source="mergeSource"
    @merged="selectedId = $event"
  />

  <ShadcnAlertDialog
    :open="deleting !== null"
    @update:open="(value: boolean) => !value && (deleting = null)"
  >
    <ShadcnAlertDialogContent v-if="deleting">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('passwords.tags.deleteTitle')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{
            t('passwords.tags.deleteBody', {
              name: deleting.name,
              count: deleting.itemCount,
            })
          }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('passwords.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          data-testid="passwords-tag-delete-confirm"
          @click="confirmDeleteAsync"
          >{{ t('passwords.tags.delete') }}</UiButton
        >
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
