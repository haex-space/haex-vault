<script setup lang="ts">
/**
 * The passkeys of an entry (spec 034, FR-004; spec 036, US5, research R6): nickname or relying
 * party, relying party, user, created and last used, and a mark for a passkey that signs in
 * without a user name. Its own passkeys can be renamed and deleted (with a confirmation naming the
 * relying party). A passkey of another entry shown by a link reads "Verweis auf <Quelle>" (a tap
 * opens the source) and offers only "Verweis lösen". There is no button to create one and a key is
 * never shown.
 */
import { toast } from 'vue-sonner'
import type { PasskeyView } from '@bindings/PasskeyView'
import { displayTitle, relativeTime } from '~/lib/passwords/format'

const props = defineProps<{
  itemId: string
  passkeys: PasskeyView[]
  readonly?: boolean
}>()

const emit = defineEmits<{
  changed: []
}>()

const { t, d, locale } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const { renamePasskeyAsync, deletePasskeyAsync, passkeyUnlinkAsync } =
  usePasswords()

const editing = ref<string | null>(null)
const nickname = ref('')
const deleting = ref<PasskeyView | null>(null)

const ALGORITHMS: Record<number, string> = {
  [-7]: 'ES256',
  [-8]: 'EdDSA',
  [-257]: 'RS256',
}

function algorithmLabel(algorithm: number): string {
  return (
    ALGORITHMS[algorithm] ??
    t('passwords.passkeys.otherAlgorithm', { number: algorithm })
  )
}

function created(stamp: string | null): string {
  if (!stamp) return '–'
  const date = new Date(stamp)
  return Number.isNaN(date.getTime()) ? stamp : d(date, { dateStyle: 'medium' })
}

function ago(stamp: string): string {
  const date = new Date(stamp)
  return Number.isNaN(date.getTime())
    ? stamp
    : relativeTime(date, new Date(), locale.value)
}

function sourceTitle(passkey: PasskeyView): string {
  return displayTitle(passkey.linkedFrom?.title) ?? t('passwords.untitled')
}

function startRename(passkey: PasskeyView) {
  if (props.readonly) return
  editing.value = passkey.id
  nickname.value = passkey.nickname ?? ''
}

async function saveRenameAsync(passkey: PasskeyView) {
  if (props.readonly) return
  try {
    await renamePasskeyAsync(passkey.id, nickname.value.trim() || null)
    editing.value = null
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function unlinkAsync(passkey: PasskeyView) {
  if (props.readonly) return
  try {
    await passkeyUnlinkAsync(props.itemId, passkey.id)
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function confirmDeleteAsync() {
  if (props.readonly) return
  const target = deleting.value
  if (!target) return
  deleting.value = null
  try {
    await deletePasskeyAsync(target.id)
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  }
}
</script>

<template>
  <SettingsGroup v-if="passkeys.length" :label="t('passwords.passkeys.title')">
    <SettingsRow
      v-for="passkey in passkeys"
      :key="passkey.id"
      :title="
        passkey.nickname || passkey.relyingPartyName || passkey.relyingPartyId
      "
      :description="
        [
          passkey.relyingPartyId,
          passkey.userName,
          algorithmLabel(passkey.algorithm),
        ]
          .filter(Boolean)
          .join(' · ')
      "
      :icon="passkey.linkedFrom ? 'lucide:link' : 'lucide:fingerprint'"
      :data-testid="`passwords-passkey-row-${passkey.id}`"
    >
      <template v-if="passkey.linkedFrom">
        <UiButton
          v-if="!readonly"
          type="button"
          variant="ghost"
          size="sm"
          :data-testid="`passwords-passkey-unlink-${passkey.id}`"
          @click="unlinkAsync(passkey)"
        >
          {{ t('passwords.passkeys.unlink') }}
        </UiButton>
      </template>
      <template v-else-if="!readonly && editing === passkey.id">
        <div class="w-40">
          <UiInput
            v-model="nickname"
            :aria-label="t('passwords.passkeys.nickname')"
            :data-testid="`passwords-passkey-nickname-${passkey.id}`"
            @keydown.enter.prevent="saveRenameAsync(passkey)"
            @keydown.esc.prevent="editing = null"
          />
        </div>
        <UiButton type="button" size="sm" @click="saveRenameAsync(passkey)">{{
          t('passwords.save')
        }}</UiButton>
      </template>
      <template v-else-if="!readonly">
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          :aria-label="t('passwords.passkeys.rename')"
          :tooltip="t('passwords.passkeys.rename')"
          :data-testid="`passwords-passkey-rename-${passkey.id}`"
          @click="startRename(passkey)"
        >
          <Icon name="lucide:pencil" class="size-4" />
        </UiButton>
        <UiButton
          type="button"
          variant="ghost"
          size="icon"
          :aria-label="t('passwords.passkeys.delete')"
          :tooltip="t('passwords.passkeys.delete')"
          :data-testid="`passwords-passkey-delete-${passkey.id}`"
          @click="deleting = passkey"
        >
          <Icon name="lucide:trash-2" class="size-4" />
        </UiButton>
      </template>
      <template #below>
        <div
          class="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground"
          :data-testid="`passwords-passkey-${passkey.id}`"
        >
          <button
            v-if="passkey.linkedFrom"
            type="button"
            class="inline-flex items-center gap-0.5 rounded-full border border-primary/40 bg-primary/10 px-1.5 text-foreground hover:bg-primary/20"
            :data-testid="`passwords-passkey-source-${passkey.id}`"
            @click="router.push(`/entry/${passkey.linkedFrom.itemId}`)"
          >
            <Icon name="lucide:link" class="size-3" />
            {{
              t('passwords.passkeys.linkedFrom', {
                source: sourceTitle(passkey),
              })
            }}
          </button>
          <span>{{
            t('passwords.passkeys.created', {
              date: created(passkey.createdAt),
            })
          }}</span>
          <span v-if="passkey.lastUsedAt">
            ·
            {{
              t('passwords.passkeys.lastUsed', {
                date: ago(passkey.lastUsedAt),
              })
            }}
          </span>
          <span
            v-if="passkey.isDiscoverable"
            class="rounded-full bg-muted px-1.5"
            :data-testid="`passwords-passkey-discoverable-${passkey.id}`"
          >
            {{ t('passwords.passkeys.discoverable') }}
          </span>
        </div>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <ShadcnAlertDialog
    :open="deleting !== null"
    @update:open="(open: boolean) => !open && (deleting = null)"
  >
    <ShadcnAlertDialogContent v-if="deleting">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('passwords.passkeys.deleteTitle')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{
            t('passwords.passkeys.deleteBody', {
              name: deleting.relyingPartyName || deleting.relyingPartyId,
            })
          }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('passwords.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          type="button"
          variant="destructive"
          data-testid="passwords-passkey-delete-confirm"
          @click="confirmDeleteAsync"
          >{{ t('passwords.passkeys.delete') }}</UiButton
        >
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
