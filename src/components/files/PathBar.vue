<script setup lang="ts">
/**
 * The path of one file browser panel (spec 044 FR-003, FR-004): up, the crumbs of the open folder
 * and reload. In the toolbar while there is one panel, in the panel's own header with two
 * (spec 062).
 */
import { breadcrumbs } from '~/lib/files/state'

const props = defineProps<{
  path: string | null
  canGoUp: boolean
  /** The name the root shows in the path bar (a storage's name instead of `/`). */
  rootLabel?: string
}>()

const emit = defineEmits<{
  go: [path: string]
  up: []
  reload: []
}>()

const { t } = useI18n()

const crumbs = computed(() => (props.path ? breadcrumbs(props.path) : []))
</script>

<template>
  <div class="flex min-w-0 flex-1 items-center gap-1">
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.up')"
      :tooltip="t('files.up')"
      :disabled="!canGoUp"
      data-testid="files-up"
      @click="emit('up')"
    >
      <Icon name="lucide:arrow-up" class="size-4" />
    </UiButton>

    <nav
      class="min-w-0 flex-1 overflow-x-auto"
      :aria-label="t('files.pathBar')"
      data-testid="files-path-bar"
    >
      <ShadcnBreadcrumbList class="flex-nowrap">
        <template v-for="(crumb, index) in crumbs" :key="crumb.path">
          <ShadcnBreadcrumbSeparator v-if="index > 0" />
          <ShadcnBreadcrumbItem class="shrink-0">
            <ShadcnBreadcrumbLink
              as="button"
              type="button"
              class="rounded px-1"
              :aria-current="index === crumbs.length - 1 ? 'page' : undefined"
              :data-testid="`files-crumb-${index}`"
              @click="emit('go', crumb.path)"
            >
              {{ index === 0 && rootLabel ? rootLabel : crumb.name }}
            </ShadcnBreadcrumbLink>
          </ShadcnBreadcrumbItem>
        </template>
      </ShadcnBreadcrumbList>
    </nav>

    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.reload')"
      :tooltip="t('files.reload')"
      data-testid="files-reload"
      @click="emit('reload')"
    >
      <Icon name="lucide:refresh-cw" class="size-4" />
    </UiButton>
  </div>
</template>
