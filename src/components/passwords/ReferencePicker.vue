<script setup lang="ts">
/**
 * Picks the value a reference points at (spec 036, US7, research R11): first an entry from the
 * headers the window holds (searched like the list), then its user name, its password or one of
 * its custom fields by key. The placeholder comes from the backend (`passwords_reference_token`);
 * no value is loaded.
 */
import { toast } from 'vue-sonner'
import type { RefMarkKind } from '@bindings/RefMarkKind'
import { displayTitle } from '~/lib/passwords/format'
import { entryTypeOf } from '~/lib/passwords/passType'
import { filterHeaders, placeholderParts } from '~/lib/passwords/search'
import { isInTrash, trashGroupIds } from '~/lib/passwords/tree'

const props = defineProps<{
  /** The entry being edited, left out of the list; `null` for a new one. */
  itemId: string | null
}>()

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  pick: [token: string]
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { errString } = useErrorString()
const store = usePasswordsStore()
const { itemKeyNamesAsync, referenceTokenAsync } = usePasswords()

const query = ref('')
const sourceId = ref<string | null>(null)
const keys = ref<string[]>([])

watch(open, (isOpen) => {
  if (!isOpen) return
  query.value = ''
  sourceId.value = null
  keys.value = []
})

const candidates = computed(() => {
  const trash = trashGroupIds(store.groups)
  const live = store.headers.filter(
    (header) =>
      header.id !== props.itemId &&
      // A reference resolves the fields of a login; a pass has none (spec 092, FR-017).
      entryTypeOf(header) === 'login' &&
      !isInTrash(header.groupId, trash),
  )
  return filterHeaders(live, { query: query.value }).slice(0, 50)
})

/** The user name without raw placeholders; a reference reads as a mark word. */
function usernameLabel(username: string | null): string | undefined {
  if (!username) return undefined
  return placeholderParts(username)
    .map((part) =>
      part.kind === 'text'
        ? part.text
        : `[${t('passwords.references.listMark')}]`,
    )
    .join('')
}

const source = computed(() =>
  sourceId.value ? store.headersById.get(sourceId.value) : undefined,
)

let keysRequest = 0

async function chooseAsync(id: string) {
  const mine = ++keysRequest
  sourceId.value = id
  keys.value = []
  try {
    const names = await itemKeyNamesAsync(id)
    if (mine === keysRequest) keys.value = names
  } catch (cause) {
    if (mine === keysRequest) toast.error(errString(cause))
  }
}

function back() {
  keysRequest++
  sourceId.value = null
  keys.value = []
}

async function pickAsync(kind: RefMarkKind, key?: string) {
  if (!sourceId.value) return
  try {
    emit('pick', await referenceTokenAsync(sourceId.value, kind, key))
    open.value = false
  } catch (cause) {
    toast.error(errString(cause))
  }
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('passwords.references.insert')">
    <template #content>
      <div class="flex flex-col gap-3" data-testid="passwords-reference-picker">
        <template v-if="!source">
          <UiInput
            v-model="query"
            type="search"
            :label="t('passwords.references.searchEntry')"
            :labels="fieldLabels.input.value"
            data-testid="passwords-reference-search"
          />
          <p
            v-if="candidates.length === 0"
            class="py-4 text-center text-sm text-muted-foreground"
          >
            {{ t('passwords.noMatches') }}
          </p>
          <SettingsGroup v-else>
            <SettingsRow
              v-for="header in candidates"
              :key="header.id"
              :title="displayTitle(header.title) ?? t('passwords.untitled')"
              :description="usernameLabel(header.username)"
              navigates
              :data-testid="`passwords-reference-source-${header.id}`"
              @click="chooseAsync(header.id)"
            />
          </SettingsGroup>
        </template>
        <template v-else>
          <div class="flex items-center gap-2">
            <UiButton
              type="button"
              variant="ghost"
              size="icon-sm"
              :aria-label="t('passwords.back')"
              @click="back"
            >
              <Icon name="lucide:arrow-left" class="size-4" />
            </UiButton>
            <span class="min-w-0 truncate font-medium">{{
              displayTitle(source.title) ?? t('passwords.untitled')
            }}</span>
          </div>
          <SettingsGroup :label="t('passwords.references.pickValue')">
            <SettingsRow
              :title="t('passwords.fields.username')"
              navigates
              data-testid="passwords-reference-value-username"
              @click="pickAsync('username')"
            />
            <SettingsRow
              :title="t('passwords.fields.password')"
              navigates
              data-testid="passwords-reference-value-password"
              @click="pickAsync('password')"
            />
            <SettingsRow
              v-for="key in keys"
              :key="key"
              :title="key"
              navigates
              :data-testid="`passwords-reference-value-extra-${key}`"
              @click="pickAsync('extra', key)"
            />
          </SettingsGroup>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>
