<template>
  <div class="flex items-center gap-3 px-3 py-2">
    <div class="shrink-0 size-9 rounded-full bg-primary/10 flex items-center justify-center">
      <UIcon
        name="i-lucide-key-round"
        class="size-4 text-primary"
      />
    </div>

    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2">
        <template v-if="isEditingNickname">
          <UInput
            ref="nicknameInputRef"
            v-model="editedNickname"
            size="sm"
            :placeholder="t('nickname.placeholder')"
            class="flex-1"
            @keyup.enter="saveNickname"
            @keyup.escape="cancelNickname"
            @blur="saveNickname"
          />
        </template>
        <span
          v-else
          class="font-medium text-sm truncate"
          :class="{ 'cursor-pointer hover:text-primary': !readOnly }"
          @click="!readOnly && startEditNickname()"
        >
          {{ displayName }}
        </span>
      </div>

      <div class="flex items-center gap-2 text-xs text-muted mt-0.5">
        <span class="truncate">{{ passkey.relyingPartyId }}</span>
        <span
          v-if="passkey.userName"
          class="truncate"
        >· {{ passkey.userName }}</span>
      </div>

      <div
        v-if="passkey.createdAt || passkey.lastUsedAt || passkey.isDiscoverable"
        class="flex items-center gap-3 text-xs text-muted mt-1"
      >
        <span
          v-if="passkey.createdAt"
          class="flex items-center gap-1"
        >
          <UIcon
            name="i-lucide-calendar"
            class="size-3"
          />
          {{ formatDate(passkey.createdAt) }}
        </span>
        <span
          v-if="passkey.lastUsedAt"
          class="flex items-center gap-1"
        >
          <UIcon
            name="i-lucide-clock"
            class="size-3"
          />
          {{ formatRelativeTime(passkey.lastUsedAt) }}
        </span>
        <span
          v-if="passkey.isDiscoverable"
          class="flex items-center gap-1 text-primary"
        >
          <UIcon
            name="i-lucide-fingerprint"
            class="size-3"
          />
          {{ t('discoverable') }}
        </span>
      </div>
    </div>

    <UiButton
      v-if="!readOnly"
      icon="i-lucide-trash-2"
      color="error"
      variant="ghost"
      type="button"
      @click="showDeleteConfirm = true"
    />
  </div>

  <UModal
    v-model:open="showDeleteConfirm"
    :title="t('deleteDialog.title')"
    :description="t('deleteDialog.description', { name: displayName })"
  >
    <template #footer>
      <div class="flex flex-col sm:flex-row gap-2 justify-end w-full">
        <UiButton
          icon="i-lucide-x"
          :label="t('deleteDialog.cancel')"
          color="neutral"
          variant="outline"
          type="button"
          @click="showDeleteConfirm = false"
        />
        <UiButton
          icon="i-lucide-trash-2"
          :label="t('deleteDialog.confirm')"
          color="error"
          variant="solid"
          type="button"
          @click="onDelete"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type { SelectHaexPasswordsPasskeys } from '~/database/schemas'

const props = defineProps<{
  passkey: SelectHaexPasswordsPasskeys
  readOnly?: boolean
}>()

const emit = defineEmits<{
  delete: [passkeyId: string]
  updateNickname: [passkeyId: string, nickname: string]
}>()

const { t, locale } = useI18n()

const showDeleteConfirm = ref(false)
const isEditingNickname = ref(false)
const editedNickname = ref('')
const nicknameInputRef = ref<{ $el?: HTMLElement } | null>(null)

const displayName = computed(
  () =>
    props.passkey.nickname ||
    props.passkey.relyingPartyName ||
    props.passkey.relyingPartyId,
)

const formatDate = (dateString: string) =>
  new Date(dateString).toLocaleDateString(locale.value, { dateStyle: 'medium' })

const formatRelativeTime = (dateString: string) => {
  const diffDays = Math.floor(
    (Date.now() - new Date(dateString).getTime()) / 86400000,
  )
  if (diffDays === 0) return t('time.today')
  if (diffDays === 1) return t('time.yesterday')
  if (diffDays < 7) return t('time.daysAgo', { days: diffDays })
  return formatDate(dateString)
}

const startEditNickname = () => {
  editedNickname.value = props.passkey.nickname || ''
  isEditingNickname.value = true
  nextTick(() =>
    nicknameInputRef.value?.$el?.querySelector('input')?.focus(),
  )
}

const saveNickname = () => {
  if (editedNickname.value !== props.passkey.nickname) {
    emit('updateNickname', props.passkey.id, editedNickname.value)
  }
  isEditingNickname.value = false
}

const cancelNickname = () => {
  isEditingNickname.value = false
}

const onDelete = () => {
  emit('delete', props.passkey.id)
  showDeleteConfirm.value = false
}
</script>

<i18n lang="yaml">
de:
  discoverable: Auffindbar
  nickname:
    placeholder: Nickname eingeben...
  time:
    today: Heute
    yesterday: Gestern
    daysAgo: Vor {days} Tagen
  deleteDialog:
    title: Passkey löschen?
    description: Der Passkey "{name}" wird unwiderruflich gelöscht.
    cancel: Abbrechen
    confirm: Löschen

en:
  discoverable: Discoverable
  nickname:
    placeholder: Enter nickname...
  time:
    today: Today
    yesterday: Yesterday
    daysAgo: "{days} days ago"
  deleteDialog:
    title: Delete passkey?
    description: The passkey "{name}" will be permanently deleted.
    cancel: Cancel
    confirm: Delete
</i18n>
