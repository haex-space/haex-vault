<script setup lang="ts">
/**
 * The tag manager (spec 034, US2, FR-011): rename, color and delete tags for every entry that
 * carries them. A name that exists already (ignoring case and the form of umlauts) is refused.
 * Deleting asks first, and only the user can do any of this (FR-028).
 */
import { toast } from 'vue-sonner'
import type { TagRow } from '@bindings/TagRow'

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { errString } = useErrorString()
const store = usePasswordsStore()
const { renameTagAsync, setTagColorAsync, deleteTagAsync } = usePasswords()

const editing = ref<string | null>(null)
const name = ref('')
const error = ref<string | null>(null)
const deleting = ref<TagRow | null>(null)

// The hint refers to the name that was refused: it goes away once the name is changed.
watch(name, () => {
  error.value = null
})

function start(tag: TagRow) {
  editing.value = tag.id
  name.value = tag.name
  error.value = null
}

async function renameAsync(tag: TagRow) {
  try {
    await renameTagAsync(tag.id, name.value)
    editing.value = null
    error.value = null
    await store.quietReloadAsync()
  } catch (cause) {
    // A refused name belongs to the name field; any other failure is a toast.
    const reason = (cause as { reason?: string }).reason
    if (reason === 'exists' || reason === 'empty' || reason === 'too_long')
      error.value = t(`passwords.tags.errors.${reason}`)
    else toast.error(errString(cause))
  }
}

async function colorAsync(tag: TagRow, color: string | null) {
  try {
    await setTagColorAsync(tag.id, color)
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
    await deleteTagAsync(tag.id)
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
        v-if="store.tags.length === 0"
        class="py-4 text-sm text-muted-foreground"
      >
        {{ t('passwords.tags.none') }}
      </p>
      <SettingsGroup v-else>
        <li
          v-for="tag in store.tags"
          :key="tag.id"
          class="px-4 py-2.5"
          :data-testid="`passwords-tag-${tag.id}`"
        >
          <div class="flex flex-wrap items-center gap-2">
            <template v-if="editing === tag.id">
              <div class="min-w-32 flex-1">
                <UiInput
                  v-model="name"
                  :aria-label="t('passwords.tags.name')"
                  :data-testid="`passwords-tag-name-${tag.id}`"
                  @keydown.enter.prevent="renameAsync(tag)"
                  @keydown.esc.prevent="editing = null"
                />
                <p
                  v-if="error"
                  class="mt-1 text-sm text-destructive"
                  role="alert"
                >
                  {{ error }}
                </p>
              </div>
              <UiButton size="sm" @click="renameAsync(tag)">{{
                t('passwords.save')
              }}</UiButton>
            </template>
            <template v-else>
              <span
                class="min-w-0 flex-1 truncate"
                :style="tag.color ? { color: tag.color } : undefined"
                >{{ tag.name }}</span
              >
              <span class="text-sm text-muted-foreground">{{
                t(
                  'passwords.tags.count',
                  { count: tag.itemCount },
                  tag.itemCount,
                )
              }}</span>
              <PasswordsColorSelect
                :model-value="tag.color"
                :data-testid="`passwords-tag-color-${tag.id}`"
                @update:model-value="colorAsync(tag, $event)"
              />
              <UiButton
                type="button"
                variant="ghost"
                size="icon"
                class="size-8 text-muted-foreground hover:bg-background hover:text-foreground focus-visible:text-foreground"
                :aria-label="t('passwords.tags.rename')"
                :tooltip="t('passwords.tags.rename')"
                :data-testid="`passwords-tag-rename-${tag.id}`"
                @click="start(tag)"
              >
                <Icon name="lucide:pencil" class="size-4" aria-hidden="true" />
              </UiButton>
              <DeleteButton
                :label="t('passwords.tags.delete')"
                :data-testid="`passwords-tag-delete-${tag.id}`"
                @click="deleting = tag"
              />
            </template>
          </div>
        </li>
      </SettingsGroup>
    </template>
  </UiDrawerModal>

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
        <UiButton variant="destructive" @click="confirmDeleteAsync">{{
          t('passwords.tags.delete')
        }}</UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
