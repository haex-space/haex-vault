<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
  >
    <template #body>
      <UiInput
        v-model="form.name"
        :label="t('nameLabel')"
        @keydown.enter.prevent="onSubmit"
      />
      <div class="space-y-2">
        <label class="text-sm font-medium">{{ t('serverLabel') }}</label>
        <div class="flex items-center gap-2">
          <USelectMenu
            v-model="form.originUrl"
            :items="serverOptions"
            :placeholder="t('serverPlaceholder')"
            :disabled="spaceIsLocal"
            class="flex-1"
          />
          <UiButton
            icon="i-lucide-server"
            variant="outline"
            color="neutral"
            @click="emit('navigate-to-sync')"
          />
        </div>
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
          icon="i-lucide-save"
          :loading="submitting"
          :disabled="!form.name?.trim()"
          @click="onSubmit"
        >
          {{ t('submit') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { SpaceWithType } from '@/stores/spaces'

type ServerOption = { label: string; value: string }

export interface EditSpacePayload {
  name: string
  originUrl: string
}

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  space: SpaceWithType | null
  serverOptions: ServerOption[]
  spaceIsLocal: boolean
  submitting: boolean
}>()

const emit = defineEmits<{
  submit: [payload: EditSpacePayload]
  'navigate-to-sync': []
}>()

const { t } = useI18n()

const form = reactive({
  name: '',
  originUrl: undefined as ServerOption | undefined,
})

// Seed form from the incoming space whenever the dialog opens.
watch(
  () => [open.value, props.space] as const,
  ([isOpen, space]) => {
    if (!isOpen || !space) return
    form.name = space.name
    form.originUrl = space.originUrl
      ? props.serverOptions.find((o) => o.value === space.originUrl)
      : props.serverOptions[0]
  },
)

const onSubmit = () => {
  if (!form.name.trim()) return
  emit('submit', {
    name: form.name.trim(),
    originUrl: form.originUrl?.value ?? '',
  })
}
</script>

<i18n lang="yaml">
de:
  title: Space bearbeiten
  nameLabel: Name
  serverLabel: Sync-Server
  serverPlaceholder: Server auswählen
  submit: Speichern
  cancel: Abbrechen
en:
  title: Edit Space
  nameLabel: Name
  serverLabel: Sync Server
  serverPlaceholder: Select server
  submit: Save
  cancel: Cancel
</i18n>
