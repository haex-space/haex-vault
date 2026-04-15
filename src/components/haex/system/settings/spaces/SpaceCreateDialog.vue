<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <UiInput
        v-model="form.name"
        :label="t('nameLabel')"
        data-testid="spaces-create-name"
        @keydown.enter.prevent="onSubmit"
      />

      <!-- Type selector -->
      <div class="space-y-1.5">
        <label class="text-sm font-medium">{{ t('typeLabel') }}</label>
        <div class="grid grid-cols-2 gap-2">
          <button
            data-testid="spaces-create-type-local"
            class="flex flex-col items-center gap-1.5 p-3 rounded-lg border transition-colors"
            :class="
              form.type === SpaceType.LOCAL
                ? 'border-primary bg-primary/5 text-primary'
                : 'border-default hover:border-primary/50'
            "
            @click="form.type = SpaceType.LOCAL"
          >
            <UIcon
              name="i-lucide-hard-drive"
              class="w-5 h-5"
            />
            <span class="text-sm font-medium">{{ t('typeLocal') }}</span>
            <span class="text-xs text-muted text-center">{{
              t('typeLocalHint')
            }}</span>
          </button>
          <button
            data-testid="spaces-create-type-online"
            class="flex flex-col items-center gap-1.5 p-3 rounded-lg border transition-colors"
            :class="
              form.type === SpaceType.ONLINE
                ? 'border-primary bg-primary/5 text-primary'
                : 'border-default hover:border-primary/50'
            "
            @click="form.type = SpaceType.ONLINE"
          >
            <UIcon
              name="i-lucide-cloud"
              class="w-5 h-5"
            />
            <span class="text-sm font-medium">{{ t('typeOnline') }}</span>
            <span class="text-xs text-muted text-center">{{
              t('typeOnlineHint')
            }}</span>
          </button>
        </div>
      </div>

      <UiSelectMenu
        v-model="form.ownerIdentityId"
        :items="ownerIdentityOptions"
        :label="t('ownerLabel')"
        value-key="value"
      />

      <!-- Server selector (only for online) -->
      <div
        v-if="form.type === SpaceType.ONLINE"
        class="flex items-center gap-2"
      >
        <UiSelectMenu
          v-model="form.serverUrl"
          :items="serverUrlOptions"
          :label="t('serverLabel')"
          class="flex-1"
        />
        <UiButton
          icon="i-lucide-server"
          variant="outline"
          color="neutral"
          @click="emit('navigate-to-sync')"
        />
      </div>
    </template>

    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UiButton
          icon="i-lucide-plus"
          :loading="submitting"
          :disabled="!form.name?.trim() || !form.ownerIdentityId"
          data-testid="spaces-create-submit"
          @click="onSubmit"
        >
          {{ t('submit') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { SpaceType, type SpaceType as SpaceTypeValue } from '~/database/constants'

type ServerOption = { label: string; value: string }
type IdentityOption = { label: string; value: string }

export interface CreateSpacePayload {
  name: string
  type: SpaceTypeValue
  ownerIdentityId: string
  serverUrl?: ServerOption
}

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  serverUrlOptions: ServerOption[]
  ownerIdentityOptions: IdentityOption[]
  defaultOwnerIdentityId?: string
  submitting: boolean
}>()

const emit = defineEmits<{
  submit: [payload: CreateSpacePayload]
  'navigate-to-sync': []
}>()

const { t } = useI18n()

const form = reactive({
  name: '',
  type: SpaceType.LOCAL as SpaceTypeValue,
  ownerIdentityId: '',
  serverUrl: undefined as ServerOption | undefined,
})

// Reset form whenever the dialog opens so stale values don't leak across uses.
watch(open, (isOpen) => {
  if (isOpen) {
    form.name = ''
    form.type = SpaceType.LOCAL
    form.ownerIdentityId = props.defaultOwnerIdentityId || props.ownerIdentityOptions[0]?.value || ''
    form.serverUrl = undefined
  }
})

const onSubmit = () => {
  if (!form.name.trim()) return
  emit('submit', {
    name: form.name.trim(),
    type: form.type,
    ownerIdentityId: form.ownerIdentityId,
    serverUrl: form.serverUrl,
  })
}
</script>

<i18n lang="yaml">
de:
  title: Space erstellen
  description: Erstelle einen neuen geteilten Space
  nameLabel: Name
  typeLabel: Typ
  typeLocal: Lokal
  typeOnline: Online
  typeLocalHint: Daten bleiben auf deinen Geräten
  typeOnlineHint: Synchronisiert über einen Server
  ownerLabel: Besitzer-Identität
  serverLabel: Sync-Server
  submit: Erstellen
  cancel: Abbrechen
en:
  title: Create Space
  description: Create a new shared space
  nameLabel: Name
  typeLabel: Type
  typeLocal: Local
  typeOnline: Online
  typeLocalHint: Data stays on your devices
  typeOnlineHint: Synchronized via a server
  ownerLabel: Owner Identity
  serverLabel: Sync Server
  submit: Create
  cancel: Cancel
</i18n>
