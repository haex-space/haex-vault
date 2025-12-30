<template>
  <!-- Hide workspace drawer on small screens - workspaces are desktop-only -->
  <UiDrawer
    v-if="!isSmallScreen"
    v-model:open="isOverviewMode"
    direction="left"
    :overlay="false"
    :modal="false"
    :title="t('title')"
    :description="t('description')"
  >
    <template #header>
      <div class="flex items-center justify-between">
        <div>
          <h3 class="text-lg font-semibold">{{ t('title') }}</h3>
          <p class="text-sm text-muted">{{ t('description') }}</p>
        </div>
        <UButton
          icon="i-heroicons-x-mark"
          color="neutral"
          variant="ghost"
          size="lg"
          square
          @click="isOverviewMode = false"
        />
      </div>
    </template>

    <template #body>
      <div class="pl-8 pr-4 py-8">
        <!-- Workspace Cards -->
        <div class="flex flex-col gap-3">
          <HaexWorkspaceCard
            v-for="workspace in workspaces"
            :key="workspace.id"
            :workspace
          />
        </div>

        <!-- Add New Workspace Button -->
        <UButton
          block
          variant="outline"
          class="mt-6"
          icon="i-heroicons-plus"
          :label="t('add')"
          @click="handleAddWorkspaceAsync"
        />
      </div>
    </template>
  </UiDrawer>
</template>

<script setup lang="ts">
const { t } = useI18n()

const workspaceStore = useWorkspaceStore()
const uiStore = useUiStore()
const { workspaces, isOverviewMode } = storeToRefs(workspaceStore)
const { isSmallScreen } = storeToRefs(uiStore)

const handleAddWorkspaceAsync = async () => {
  const workspace = await workspaceStore.addWorkspaceAsync()
  nextTick(() => {
    workspaceStore.slideToWorkspace(workspace?.id)
  })
}
</script>

<i18n lang="yaml">
de:
  title: Workspaces
  description: Übersicht aller Workspaces
  add: Workspace hinzufügen
en:
  title: Workspaces
  description: Overview of all workspaces
  add: Add Workspace
</i18n>
