<template>
  <div
    v-if="menuEntry"
    class="flex flex-col gap-2 p-3 rounded-lg border border-default bg-default"
  >
    <div class="flex items-center gap-2">
      <div class="flex-1 min-w-0">
        <!-- Edit mode -->
        <UInput
          v-if="isEditing"
          ref="inputRef"
          v-model="localTarget"
          class="font-medium w-full"
          :placeholder="t('targetPlaceholder')"
          @keydown.enter="finishEditing"
          @keydown.escape="isEditing = false"
        />
        <!-- Display mode -->
        <div
          v-else
          class="font-medium break-all inline-flex items-center gap-1"
        >
          <UPopover
            v-if="isWildcard"
            mode="hover"
          >
            <button
              type="button"
              class="inline-flex items-center shrink-0"
            >
              <UIcon
                name="i-heroicons-exclamation-triangle"
                class="size-4 text-orange-600 dark:text-orange-400"
              />
            </button>
            <template #content>
              <div class="p-2 text-sm max-w-64">
                {{ t('wildcardWarning') }}
              </div>
            </template>
          </UPopover>
          <span
            v-for="(part, index) in targetParts"
            :key="index"
            :class="part.isWildcard ? 'text-orange-600 dark:text-orange-400 font-bold' : ''"
          >{{ part.text }}</span>
        </div>
        <div
          v-if="permissionEntry.operation && !isEditing"
          class="text-sm text-gray-500 dark:text-gray-400"
        >
          {{ t(`operation.${permissionEntry.operation}`) }}
        </div>
      </div>

      <!-- Edit button -->
      <UiButton
        :icon="isEditing ? 'i-heroicons-check' : 'i-heroicons-pencil'"
        :color="isEditing ? 'success' : 'neutral'"
        :variant="isEditing ? 'solid' : 'ghost'"
        :title="isEditing ? t('confirmEdit') : t('editTarget')"
        @click="toggleEditing"
      />
    </div>

    <div class="flex items-center">
      <!-- Status Selector -->
      <USelectMenu
        v-model="menuEntry"
        :items="statusOptions"
        class="w-full sm:w-44"
        :search-input="false"
      >
        <template #leading>
          <UIcon
            :name="getStatusIcon(menuEntry?.value)"
            :class="getStatusColor(menuEntry?.value)"
          />
        </template>

        <template #item-leading="{ item }">
          <UIcon
            :name="getStatusIcon(item?.value)"
            :class="getStatusColor(item?.value)"
          />
        </template>
      </USelectMenu>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { PermissionEntry } from '~~/src-tauri/bindings/PermissionEntry'
import type { PermissionStatus } from '~~/src-tauri/bindings/PermissionStatus'

const permissionEntry = defineModel<PermissionEntry>({ required: true })

// Check if target contains wildcard (full wildcard or pattern like *.example.com)
const isWildcard = computed(() => permissionEntry.value.target.includes('*'))

// Split target into parts, highlighting wildcards
const targetParts = computed(() => {
  const target = permissionEntry.value.target
  const parts: Array<{ text: string; isWildcard: boolean }> = []

  let currentPart = ''
  for (const char of target) {
    if (char === '*') {
      // Save any accumulated non-wildcard text
      if (currentPart) {
        parts.push({ text: currentPart, isWildcard: false })
        currentPart = ''
      }
      // Add the wildcard as a separate part
      parts.push({ text: '*', isWildcard: true })
    } else {
      currentPart += char
    }
  }

  // Add remaining text
  if (currentPart) {
    parts.push({ text: currentPart, isWildcard: false })
  }

  return parts
})

const isEditing = ref(false)
const inputRef = ref<{ input: HTMLInputElement } | null>(null)
const localTarget = ref('')

const toggleEditing = () => {
  if (isEditing.value) {
    // Save changes when closing edit mode
    permissionEntry.value.target = localTarget.value
  } else {
    // Load current value when entering edit mode
    localTarget.value = permissionEntry.value.target
  }
  isEditing.value = !isEditing.value
  if (isEditing.value) {
    nextTick(() => {
      inputRef.value?.input?.focus()
    })
  }
}

const finishEditing = () => {
  permissionEntry.value.target = localTarget.value
  isEditing.value = false
}

const menuEntry = computed({
  get: () =>
    statusOptions.value.find(
      (option) => option.value == permissionEntry.value.status,
    ),
  set(newStatus) {
    const status =
      statusOptions.value.find((option) => option.value == newStatus?.value)
        ?.value || 'denied'
    if (isPermissionStatus(status)) {
      permissionEntry.value.status = status
    } else {
      permissionEntry.value.status = 'denied'
    }
  },
})

const { t } = useI18n()

const isPermissionStatus = (value: string): value is PermissionStatus => {
  return ['ask', 'granted', 'denied'].includes(value)
}

const statusOptions = computed(() => [
  {
    value: 'granted',
    label: t('status.granted'),
    icon: 'i-heroicons-check-circle',
    color: 'text-green-500',
  },
  {
    value: 'ask',
    label: t('status.ask'),
    icon: 'i-heroicons-question-mark-circle',
    color: 'text-yellow-500',
  },
  {
    value: 'denied',
    label: t('status.denied'),
    icon: 'i-heroicons-x-circle',
    color: 'text-red-500',
  },
])

const getStatusIcon = (status: string) => {
  const option = statusOptions.value.find((o) => o.value === status)
  return option?.icon || 'i-heroicons-question-mark-circle'
}

const getStatusColor = (status: string) => {
  const option = statusOptions.value.find((o) => o.value === status)
  return option?.color || 'text-gray-500'
}
</script>

<i18n lang="yaml">
de:
  targetPlaceholder: Ziel eingeben (z.B. *.example.com)
  editTarget: Ziel bearbeiten
  confirmEdit: Bearbeitung abschließen
  wildcardWarning: Wildcard-Berechtigung – erlaubt Zugriff auf alle Ziele, die diesem Muster entsprechen
  status:
    granted: Erlaubt
    ask: Nachfragen
    denied: Verweigert
  operation:
    '*': Alle
    read: Lesen
    write: Schreiben
    readWrite: Lesen & Schreiben
    request: Anfrage
    execute: Ausführen
en:
  targetPlaceholder: Enter target (e.g. *.example.com)
  editTarget: Edit target
  confirmEdit: Confirm edit
  wildcardWarning: Wildcard permission – allows access to all targets matching this pattern
  status:
    granted: Granted
    ask: Ask
    denied: Denied
  operation:
    '*': All
    read: Read
    write: Write
    readWrite: Read & Write
    request: Request
    execute: Execute
</i18n>
