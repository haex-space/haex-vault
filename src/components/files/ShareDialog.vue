<script setup lang="ts">
/**
 * Sharing a folder or file of this device with contacts (spec 056 US2, FR-016, FR-017): the name of
 * the share, the contacts with a stored card, and a level for each. An existing share opens with
 * its contacts; saving replaces them, and the share can be ended here. From the overview it edits a
 * given share, which may be served by another own device: its name and contacts, never its root
 * (FR-019).
 */
import type { Entry } from '@bindings/Entry'
import type { FileShare } from '@bindings/FileShare'
import type { FileShareLevel } from '@bindings/FileShareLevel'
import type { FilesContactShares } from '@bindings/FilesContactShares'
import { asFilesError } from '~/composables/useFiles'
import { useActionOrThrow } from '~/composables/useActionOrThrow'

const props = defineProps<{
  entry: Entry | null
  /** A share to edit as it is, instead of the one at `entry`. */
  share?: FileShare | null
  /** The contacts with a stored card (`Sources.contacts`). */
  contacts: readonly FilesContactShares[]
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { shareOfAsync } = useFileShares()
const saveShare = useActionOrThrow('files.shares.save')
const deleteShare = useActionOrThrow('files.shares.delete')

type Row = { vaultPubkey: string; name: string; level: FileShareLevel }

const name = ref('')
const rows = ref<Row[]>([])
const existing = ref<FileShare | null>(null)
const adding = ref('')
const error = ref<string | null>(null)
const saving = ref(false)
const loading = ref(false)
const loadFailed = ref(false)
let loadGeneration = 0

const contactName = (vault: string, fallback: string | null = null) =>
  props.contacts.find((contact) => contact.vaultPubkey === vault)
    ?.contactName ??
  fallback ??
  vault.slice(0, 8)

watch(
  [open, () => props.entry?.path, () => props.share],
  async ([isOpen, entryPath, given]) => {
    const generation = ++loadGeneration
    if (!isOpen || (!given && (!props.entry || !entryPath))) {
      loading.value = false
      return
    }
    await loadAsync(generation, given ?? null, entryPath ?? '')
  },
)

async function loadAsync(
  generation: number,
  given: FileShare | null,
  entryPath: string,
) {
  error.value = null
  loadFailed.value = false
  adding.value = ''
  name.value = given?.name ?? props.entry?.name ?? ''
  rows.value = []
  existing.value = null
  loading.value = true
  try {
    const share = given ?? (await shareOfAsync({ kind: 'device' }, entryPath))
    if (generation !== loadGeneration) return
    existing.value = share
    if (share) {
      name.value = share.name
      rows.value = share.grants.map((grant) => ({
        vaultPubkey: grant.vaultPubkey,
        name: contactName(grant.vaultPubkey, grant.contactName),
        level: grant.level,
      }))
    }
  } catch (cause) {
    if (generation === loadGeneration) {
      loadFailed.value = true
      failed(cause)
    }
  } finally {
    if (generation === loadGeneration) loading.value = false
  }
}

const levelOptions = computed(() => [
  { value: 'read', label: t('files.shares.level.read') },
  { value: 'readWrite', label: t('files.shares.level.readWrite') },
])

const addable = computed(() =>
  props.contacts
    .filter(
      (contact) =>
        !rows.value.some((row) => row.vaultPubkey === contact.vaultPubkey),
    )
    .map((contact) => ({
      value: contact.vaultPubkey,
      label: contact.contactName,
    })),
)

watch(adding, (vault) => {
  if (!vault) return
  rows.value = [
    ...rows.value,
    { vaultPubkey: vault, name: contactName(vault), level: 'read' },
  ]
  adding.value = ''
})

function setLevel(row: Row, level: unknown) {
  row.level = level === 'readWrite' ? 'readWrite' : 'read'
}

function remove(row: Row) {
  rows.value = rows.value.filter((other) => other !== row)
}

function failed(cause: unknown) {
  const code = asFilesError(cause)?.code
  error.value = code ? t(`files.error.${code}`) : t('files.shares.failed')
}

async function saveNowAsync() {
  const path = props.entry?.path ?? existing.value?.path
  if (!path || loading.value || loadFailed.value) return
  saving.value = true
  try {
    await saveShare({
      shareId: existing.value?.id,
      source: existing.value?.source ?? { kind: 'device' },
      path,
      name: name.value,
      grants: rows.value.map((row) => ({
        vaultPubkey: row.vaultPubkey,
        level: row.level,
      })),
    })
    open.value = false
  } catch (cause) {
    failed(cause)
  } finally {
    saving.value = false
  }
}

async function endAsync() {
  if (!existing.value) return
  saving.value = true
  try {
    await deleteShare({ shareId: existing.value.id })
    open.value = false
  } catch (cause) {
    failed(cause)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('files.shares.title', { name: entry?.name ?? share?.name ?? '' })"
  >
    <template #content>
      <form
        class="flex flex-col gap-4"
        data-testid="files-share-form"
        @submit.prevent="saveNowAsync"
      >
        <UiInput
          id="files-share-name"
          v-model="name"
          :label="t('files.shares.name')"
          :labels="fieldLabels.input.value"
          data-testid="files-share-name"
        />
        <section class="flex flex-col gap-2">
          <h3 class="text-sm font-medium">{{ t('files.shares.contacts') }}</h3>
          <p v-if="contacts.length === 0" class="text-sm text-muted-foreground">
            {{ t('files.shares.noContacts') }}
          </p>
          <ul class="flex flex-col gap-2">
            <li
              v-for="row in rows"
              :key="row.vaultPubkey"
              class="flex items-center gap-2"
              :data-testid="`files-share-grant-${row.name}`"
            >
              <span class="min-w-0 flex-1 truncate text-sm">{{
                row.name
              }}</span>
              <UiSelect
                :model-value="row.level"
                :options="levelOptions"
                :aria-label="t('files.shares.contacts')"
                @update:model-value="setLevel(row, $event)"
              />
              <UiButton
                type="button"
                variant="ghost"
                size="icon-sm"
                :aria-label="t('files.shares.remove')"
                :tooltip="t('files.shares.remove')"
                @click="remove(row)"
              >
                <Icon name="lucide:x" class="size-4" />
              </UiButton>
            </li>
          </ul>
          <UiSelect
            v-if="addable.length"
            v-model="adding"
            :options="addable"
            :placeholder="t('files.shares.addContact')"
            :aria-label="t('files.shares.addContact')"
            data-testid="files-share-add"
          />
        </section>
        <p
          v-if="error"
          class="text-sm text-destructive"
          role="alert"
          data-testid="files-share-error"
        >
          {{ error }}
        </p>
        <div class="flex justify-end gap-2">
          <UiButton
            v-if="existing"
            type="button"
            variant="outline"
            class="mr-auto"
            :disabled="saving || loading"
            data-testid="files-share-end"
            @click="endAsync"
            >{{ t('files.shares.delete') }}</UiButton
          >
          <UiButton
            type="button"
            variant="outline"
            :disabled="saving"
            @click="open = false"
            >{{ t('files.shares.cancel') }}</UiButton
          >
          <UiButton
            type="submit"
            :disabled="saving || loading || loadFailed"
            :loading="saving || loading"
            data-testid="files-share-save"
            >{{ t('files.shares.save') }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
</template>
