<script setup lang="ts">
/**
 * The tag manager (spec 034, US2, FR-011): rename, color and delete tags for every entry that
 * carries them. A name that exists already (ignoring case and the form of umlauts) is refused.
 * Deleting asks first, and only the user can do any of this (FR-028).
 */
import { toast } from 'vue-sonner'
import type { TagRow } from '@bindings/TagRow'
import { ENTRY_COLORS } from '~/lib/passwords/icons'
import { clearOnReselect } from '~/lib/ui/radio'

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
    await setTagColorAsync(tag.id, tag.color === color ? null : color)
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
          class="flex flex-col gap-2 px-4 py-3"
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
              <UiButton
                variant="ghost"
                size="icon"
                :aria-label="t('passwords.tags.rename')"
                @click="start(tag)"
              >
                <Icon name="lucide:pencil" class="size-4" />
              </UiButton>
              <UiButton
                variant="ghost"
                size="icon"
                :aria-label="t('passwords.tags.delete')"
                :data-testid="`passwords-tag-delete-${tag.id}`"
                @click="deleting = tag"
              >
                <Icon name="lucide:trash-2" class="size-4" />
              </UiButton>
            </template>
          </div>
          <ShadcnRadioGroup
            class="flex flex-wrap gap-1.5"
            :model-value="tag.color"
            :aria-label="t('passwords.fields.color')"
            @update:model-value="colorAsync(tag, String($event))"
          >
            <UiRadioGroupTile
              v-for="color in ENTRY_COLORS"
              :key="color"
              :value="color"
              :aria-label="color"
              class="size-5 rounded-full border-2 border-transparent data-[state=checked]:border-foreground"
              :style="{ backgroundColor: color }"
              @select="
                clearOnReselect(
                  $event,
                  tag.color === color,
                  () => void colorAsync(tag, null),
                )
              "
            />
          </ShadcnRadioGroup>
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
