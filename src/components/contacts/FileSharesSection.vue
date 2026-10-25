<script setup lang="ts">
/**
 * The file shares given to a contact (spec 056 US4, FR-021): each with the contact's level, which
 * can be changed or taken away here. Only contacts with a stored card can have any; the section
 * stays hidden while there are none.
 */
import type { FileShare } from '@bindings/FileShare'
import type { FileShareLevel } from '@bindings/FileShareLevel'
import { asFilesError } from '~/composables/useFiles'
import { useActionOrThrow } from '~/composables/useActionOrThrow'
import { onVaultTablesChanged } from '~/composables/useVaultData'

const props = defineProps<{ contactId: string }>()

const { t } = useI18n()
const sharesOf = useActionOrThrow('files.shares.ofContact')
const saveShare = useActionOrThrow('files.shares.save')
const revokeGrant = useActionOrThrow('files.shares.revoke')

const shares = ref<FileShare[]>([])
const busy = ref(false)
const error = ref<string | null>(null)

function failed(cause: unknown) {
  const code = asFilesError(cause)?.code
  error.value = code ? t(`files.error.${code}`) : t('files.shares.failed')
}

async function reloadAsync() {
  try {
    shares.value = (await sharesOf({
      contactId: props.contactId,
    })) as FileShare[]
    error.value = null
  } catch (cause) {
    failed(cause)
  }
}

watch(() => props.contactId, reloadAsync, { immediate: true })
onVaultTablesChanged(['haex_file_share*'], reloadAsync)

/** The contact's grant in `share`; every share listed here has one. */
function grantIn(share: FileShare) {
  return share.grants.find((grant) => grant.contactId === props.contactId)
}

const levelOptions = computed(() => [
  { value: 'read', label: t('files.shares.level.read') },
  { value: 'readWrite', label: t('files.shares.level.readWrite') },
])

async function run(work: () => Promise<unknown>) {
  if (busy.value) return
  busy.value = true
  try {
    await work()
    await reloadAsync()
  } catch (cause) {
    failed(cause)
  } finally {
    busy.value = false
  }
}

function setLevel(share: FileShare, value: unknown) {
  const own = grantIn(share)
  const level: FileShareLevel = value === 'readWrite' ? 'readWrite' : 'read'
  if (!own || own.level === level) return
  void run(() =>
    saveShare({
      shareId: share.id,
      source: share.source,
      path: share.path,
      name: share.name,
      grants: share.grants.map((grant) => ({
        vaultPubkey: grant.vaultPubkey,
        level: grant === own ? level : grant.level,
      })),
    }),
  )
}

function revoke(share: FileShare) {
  const own = grantIn(share)
  if (!own) return
  void run(() =>
    revokeGrant({ shareId: share.id, vaultPubkey: own.vaultPubkey }),
  )
}
</script>

<template>
  <SettingsGroup
    v-if="shares.length || error"
    :label="t('contacts.fileShares.title')"
    data-testid="contacts-file-shares"
  >
    <p v-if="error" class="px-4 py-2 text-sm text-destructive" role="alert">
      {{ error }}
    </p>
    <SettingsRow
      v-for="share in shares"
      :key="share.id"
      :title="share.name"
      :description="share.path"
      :data-testid="`contacts-file-share-${share.name}`"
    >
      <UiSelect
        :model-value="grantIn(share)?.level"
        :options="levelOptions"
        :disabled="busy"
        :aria-label="t('contacts.fileShares.level', { name: share.name })"
        @update:model-value="setLevel(share, $event)"
      />
      <UiButton
        variant="ghost"
        size="icon"
        :disabled="busy"
        :aria-label="t('contacts.fileShares.revoke', { name: share.name })"
        :tooltip="t('contacts.fileShares.revoke', { name: share.name })"
        :data-testid="`contacts-file-share-revoke-${share.name}`"
        @click="revoke(share)"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
    </SettingsRow>
  </SettingsGroup>
</template>
