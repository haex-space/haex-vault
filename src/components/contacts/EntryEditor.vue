<script setup lang="ts">
/**
 * The editor of a contact (spec 051, FR-002, FR-003, FR-007): a new one (`entry/new/edit`, in the
 * group of `?group=` when given) or an existing one. Name parts, organization and note, any number
 * of values of every known kind, the groups, and for a new one the phones it goes to (FR-018c,
 * "only in holzi" unless chosen). Saving sends only what changed (`toPatch`), so values of kinds
 * this version does not know stay as they are; the backend checks every value and says what it
 * refuses.
 */
import { toast } from 'vue-sonner'
import type { TargetInput } from '@bindings/TargetInput'
import {
  draftOf,
  emptyDraft,
  HEAD_FIELDS,
  newValue,
  toInput,
  toPatch,
  type Draft,
  type HeadField,
} from '~/lib/contacts/draft'
import { KINDS } from '~/lib/contacts/kinds'
import { NEW_CONTACT } from '~/lib/contacts/registry'

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useContactsStore()
const { getAsync, createAsync, updateAsync } = useContacts()
const { kindName } = useContactLabels()

const id = computed(() => router.route.params.id ?? NEW_CONTACT)
const isNew = computed(() => id.value === NEW_CONTACT)

const before = ref<Draft | null>(null)
const draft = ref<Draft>(emptyDraft())
const targets = ref<TargetInput[]>([])
const error = ref<string | null>(null)
const saving = ref(false)
const missing = ref(false)

async function loadAsync() {
  error.value = null
  if (isNew.value) {
    const group = router.route.query.group
    draft.value = emptyDraft(group ? [group] : [])
    draft.value.values = [newValue('phone'), newValue('email')]
    targets.value = []
    before.value = null
    return
  }
  try {
    const contact = await getAsync(id.value)
    before.value = draftOf(contact)
    draft.value = draftOf(contact)
    missing.value = false
  } catch {
    missing.value = true
  }
}
watch(id, loadAsync, { immediate: true })

/** Name fields shown at once; the rest behind "more". */
const NAME_FIELDS: readonly HeadField[] = ['givenName', 'familyName']
const MORE_FIELDS: readonly HeadField[] = HEAD_FIELDS.filter(
  (field) =>
    !NAME_FIELDS.includes(field) &&
    field !== 'note' &&
    field !== 'organization' &&
    field !== 'jobTitle',
)
const showMore = ref(false)

const form = useTemplateRef<HTMLFormElement>('form')

/** A value was just added from the menu: its first field takes the focus when the menu closes. */
let focusAdded = false

function addValue(kind: string) {
  draft.value.values = [...draft.value.values, newValue(kind)]
  focusAdded = true
}

/** The menu would hand the focus back to its button as it closes, taking it from the field the user
 * starts typing in; the new value's first field gets it instead. */
function onMenuClosed(event: Event) {
  if (!focusAdded) return
  focusAdded = false
  event.preventDefault()
  const rows = form.value?.querySelectorAll<HTMLElement>(
    '[data-testid^="contacts-editor-value-"]',
  )
  rows?.[rows.length - 1]?.querySelector<HTMLElement>('input')?.focus()
}

function removeValue(index: number) {
  draft.value.values = draft.value.values.filter((_, i) => i !== index)
}

function toggleGroup(groupId: string, member: boolean) {
  const groups = new Set(draft.value.groups)
  if (member) groups.add(groupId)
  else groups.delete(groupId)
  draft.value.groups = [...groups]
}

async function saveAsync() {
  saving.value = true
  error.value = null
  try {
    if (isNew.value) {
      const contact = await createAsync({
        ...toInput(draft.value),
        targets: targets.value,
      })
      await store.quietReloadAsync()
      await router.replace(`/entry/${contact.id}`)
    } else if (before.value) {
      const patch = toPatch(before.value, draft.value)
      if (patch) await updateAsync(id.value, patch)
      await store.quietReloadAsync()
      await router.replace(`/entry/${id.value}`)
    }
  } catch (cause) {
    error.value = errString(cause)
    toast.error(error.value)
  } finally {
    saving.value = false
  }
}

function cancel() {
  void router.replace(isNew.value ? '/' : `/entry/${id.value}`)
}
</script>

<template>
  <form
    ref="form"
    class="flex min-h-0 flex-col gap-4 p-3"
    data-testid="contacts-editor"
    @submit.prevent="saveAsync"
  >
    <div class="flex items-center gap-2">
      <h1 class="min-w-0 flex-1 truncate px-1 text-2xl font-semibold">
        {{
          isNew ? t('contacts.editor.newTitle') : t('contacts.editor.editTitle')
        }}
      </h1>
      <UiButton
        type="button"
        variant="outline"
        data-testid="contacts-editor-cancel"
        @click="cancel"
      >
        {{ t('contacts.cancel') }}
      </UiButton>
      <UiButton
        type="submit"
        :loading="saving"
        data-testid="contacts-editor-save"
      >
        {{ t('contacts.save') }}
      </UiButton>
    </div>

    <p v-if="missing" class="p-4 text-muted-foreground">
      {{ t('contacts.missing') }}
    </p>

    <template v-else>
      <p
        v-if="error"
        class="rounded-lg border border-destructive/40 p-3 text-sm text-destructive"
        role="alert"
        data-testid="contacts-editor-error"
      >
        {{ error }}
      </p>

      <SettingsGroup :label="t('contacts.editor.name')">
        <li class="grid gap-2 px-4 py-3 sm:grid-cols-2">
          <UiInput
            v-for="field in NAME_FIELDS"
            :id="`contacts-${field}`"
            :key="field"
            v-model="draft.head[field]"
            :label="t(`contacts.fields.${field}`)"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            :data-testid="`contacts-field-${field}`"
          />
          <template v-if="showMore">
            <UiInput
              v-for="field in MORE_FIELDS"
              :id="`contacts-${field}`"
              :key="field"
              v-model="draft.head[field]"
              :label="t(`contacts.fields.${field}`)"
              :labels="fieldLabels.input.value"
              label-bg="var(--muted)"
              :data-testid="`contacts-field-${field}`"
            />
          </template>
          <button
            v-else
            type="button"
            class="text-left text-sm text-primary sm:col-span-2"
            data-testid="contacts-editor-more"
            @click="showMore = true"
          >
            {{ t('contacts.editor.moreName') }}
          </button>
        </li>
      </SettingsGroup>

      <SettingsGroup :label="t('contacts.editor.work')">
        <li class="grid gap-2 px-4 py-3 sm:grid-cols-2">
          <UiInput
            v-for="field in ['organization', 'jobTitle'] as const"
            :id="`contacts-${field}`"
            :key="field"
            v-model="draft.head[field]"
            :label="t(`contacts.fields.${field}`)"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            :data-testid="`contacts-field-${field}`"
          />
        </li>
      </SettingsGroup>

      <SettingsGroup :label="t('contacts.editor.values')">
        <ContactsValueEditor
          v-for="(value, index) in draft.values"
          :key="value.id ?? `new-${index}`"
          v-model="draft.values[index]!"
          @remove="removeValue(index)"
        />
        <li class="px-4 py-3">
          <ShadcnDropdownMenu>
            <ShadcnDropdownMenuTrigger as-child>
              <UiButton
                type="button"
                variant="outline"
                data-testid="contacts-editor-add-value"
              >
                <Icon name="lucide:plus" class="size-4" />
                {{ t('contacts.editor.addValue') }}
              </UiButton>
            </ShadcnDropdownMenuTrigger>
            <ShadcnDropdownMenuContent
              align="start"
              @close-auto-focus="onMenuClosed"
            >
              <ShadcnDropdownMenuItem
                v-for="kind in KINDS"
                :key="kind.id"
                class="gap-2"
                :data-testid="`contacts-editor-add-${kind.id}`"
                @select="addValue(kind.id)"
              >
                <Icon :name="kind.icon" class="size-4" />
                {{ kindName(kind.id) }}
              </ShadcnDropdownMenuItem>
            </ShadcnDropdownMenuContent>
          </ShadcnDropdownMenu>
        </li>
      </SettingsGroup>

      <SettingsGroup :label="t('contacts.fields.note')">
        <li class="px-4 py-3">
          <UiTextarea
            id="contacts-note"
            v-model="draft.head.note"
            :label="t('contacts.fields.note')"
            label-bg="var(--muted)"
            rows="4"
            data-testid="contacts-field-note"
          />
        </li>
      </SettingsGroup>

      <ContactsTargetPicker v-if="isNew" v-model="targets" />

      <SettingsGroup
        v-if="store.groups.length"
        :label="t('contacts.editor.groups')"
      >
        <li v-for="group in store.groups" :key="group.id" class="px-4 py-3">
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="draft.groups.includes(group.id)"
              :data-testid="`contacts-editor-group-${group.id}`"
              @update:model-value="toggleGroup(group.id, $event === true)"
            />
            <span>{{ group.name }}</span>
          </label>
        </li>
      </SettingsGroup>
    </template>
  </form>
</template>
