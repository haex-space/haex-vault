<template>
  <nav
    v-if="selectedGroupId !== null"
    class="flex items-center gap-1 px-3 py-2 border-b border-default text-sm min-h-12 overflow-x-auto"
    :aria-label="t('ariaLabel')"
  >
    <button
      type="button"
      class="flex items-center gap-1.5 px-2 py-1 rounded-md text-muted hover:text-default hover:bg-elevated transition-colors shrink-0"
      @click="selectGroup(null)"
    >
      <UIcon
        name="i-lucide-key-round"
        class="size-4"
      />
      <span>{{ t('allPasswords') }}</span>
    </button>

    <template
      v-for="(group, index) in breadcrumbGroups"
      :key="group.id"
    >
      <UIcon
        name="i-lucide-chevron-right"
        class="size-4 text-muted shrink-0"
      />
      <button
        v-if="index < breadcrumbGroups.length - 1"
        type="button"
        class="px-2 py-1 rounded-md text-muted hover:text-default hover:bg-elevated transition-colors shrink-0 truncate max-w-40"
        :title="group.name ?? undefined"
        @click="selectGroup(group.id)"
      >
        {{ group.name || t('untitled') }}
      </button>
      <span
        v-else
        class="px-2 py-1 font-medium truncate max-w-48"
        :title="group.name ?? undefined"
      >
        {{ group.name || t('untitled') }}
      </span>
    </template>
  </nav>
</template>

<script setup lang="ts">
const { t } = useI18n()

const groupsStore = usePasswordsGroupsStore()
const { selectedGroupId, breadcrumbGroups } = storeToRefs(groupsStore)
const { selectGroup } = groupsStore
</script>

<i18n lang="yaml">
de:
  ariaLabel: Ordner-Pfad
  allPasswords: Alle Passwörter
  untitled: (ohne Namen)
en:
  ariaLabel: Folder path
  allPasswords: All Passwords
  untitled: (unnamed)
</i18n>
