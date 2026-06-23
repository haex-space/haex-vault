<template>
  <UiDialogConfirm
    :open="open"
    :title="title"
    :description="description"
    :confirm-label="confirmLabel"
    :confirm-disabled="confirmDisabled"
    confirm-icon="i-lucide-trash-2"
    @update:open="(value) => emit('update:open', value)"
    @confirm="emit('confirm')"
  >
    <div
      v-if="affectedSyncBackends.length > 0"
      class="mt-4 p-3 rounded-lg border border-error/40 bg-error/10 space-y-2"
    >
      <div class="flex items-start gap-2">
        <UIcon
          name="i-lucide-triangle-alert"
          class="w-5 h-5 shrink-0 text-error mt-0.5"
        />
        <div class="space-y-1">
          <p class="text-sm font-semibold text-error">
            {{ syncBackendWarningTitle }}
          </p>
          <p class="text-sm text-error/90">
            {{ syncBackendWarningBody }}
          </p>
        </div>
      </div>
      <ul class="list-disc list-inside text-sm text-error/90 pl-7">
        <li
          v-for="backend in affectedSyncBackends"
          :key="backend.id"
          class="font-medium"
        >
          {{ backend.name }}
          <span class="font-mono text-xs text-error/70">({{ backend.homeServerUrl }})</span>
        </li>
      </ul>
      <label class="flex items-start gap-2 pl-7 cursor-pointer pt-1">
        <UCheckbox
          :model-value="acceptedSyncBackendLoss"
          @update:model-value="(value) => emit('update:acceptedSyncBackendLoss', value === true)"
        />
        <span class="text-sm text-error font-medium">
          {{ syncBackendConfirm }}
        </span>
      </label>
    </div>

    <div
      v-if="affectedAdminSpaces.length > 0"
      class="mt-4 space-y-2"
    >
      <p class="text-sm font-medium text-highlighted">
        {{ adminSpacesWarning }}
      </p>
      <ul class="list-disc list-inside text-sm text-muted">
        <li
          v-for="space in affectedAdminSpaces"
          :key="space.id"
          class="font-medium"
        >
          {{ space.name }}
        </li>
      </ul>
    </div>
    <div
      v-if="affectedMemberSpaces.length > 0"
      class="mt-3 space-y-2"
    >
      <p class="text-sm text-muted">
        {{ memberSpacesInfo }}
      </p>
    </div>
  </UiDialogConfirm>
</template>

<script setup lang="ts">
import type {
  SelectHaexSpaces,
  SelectHaexSyncBackends,
} from '~/database/schemas'

const props = defineProps<{
  open: boolean
  acceptedSyncBackendLoss: boolean
  affectedSyncBackends: SelectHaexSyncBackends[]
  affectedAdminSpaces: SelectHaexSpaces[]
  affectedMemberSpaces: SelectHaexSpaces[]
  confirmLabel: string
  confirmDisabled: boolean
  // Translated strings (parent owns the i18n block).
  title: string
  description: string
  syncBackendWarningTitle: string
  syncBackendWarningBody: string
  syncBackendConfirm: string
  adminSpacesWarning: string
  memberSpacesInfo: string
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  'update:acceptedSyncBackendLoss': [value: boolean]
  confirm: []
}>()

void props
</script>
