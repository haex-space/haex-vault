<template>
  <HaexSystemSettingsLayoutSection
    :title="t('settings')"
  >
    <UiListContainer>
      <UiListItem>
        <div>
          <div class="font-medium text-sm">{{ t('displayMode') }}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            {{ t('displayModeDescription') }}
          </div>
        </div>
        <template #actions>
          <USelectMenu
            v-model="selectedDisplayMode"
            :items="displayModeOptions"
            class="w-40"
            :search-input="false"
            @update:model-value="emit('updateDisplayMode', $event)"
          />
        </template>
      </UiListItem>

      <UiListItem>
        <div>
          <div class="font-medium text-sm">{{ t('singleInstance') }}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            {{ t('singleInstanceDescription') }}
          </div>
        </div>
        <template #actions>
          <span class="text-sm">{{
            extension.singleInstance ? t('yes') : t('no')
          }}</span>
        </template>
      </UiListItem>

      <UiListItem>
        <div>
          <div class="font-medium text-sm">{{ t('id') }}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            {{ t('idDescription') }}
          </div>
        </div>
        <template #actions>
          <code
            class="text-xs bg-muted px-2 py-1 rounded break-all max-w-[50%] text-right"
          >
            {{ extension.id }}
          </code>
        </template>
      </UiListItem>

      <UiListItem v-if="extension.homepage">
        <div>
          <div class="font-medium text-sm">{{ t('homepage') }}</div>
        </div>
        <template #actions>
          <a
            :href="extension.homepage"
            target="_blank"
            class="text-sm text-primary hover:underline truncate max-w-[50%]"
          >
            {{ extension.homepage }}
          </a>
        </template>
      </UiListItem>
    </UiListContainer>
  </HaexSystemSettingsLayoutSection>
</template>

<script setup lang="ts">
import type { DisplayMode } from '~~/src-tauri/bindings/DisplayMode'
import type { IHaexSpaceExtension } from '~/types/haexspace'

export interface IDisplayModeOption {
  value: DisplayMode
  label: string
}

defineProps<{
  extension: IHaexSpaceExtension
  displayModeOptions: IDisplayModeOption[]
}>()

const selectedDisplayMode = defineModel<IDisplayModeOption>('selectedDisplayMode', { required: true })

const emit = defineEmits<{
  updateDisplayMode: [option: IDisplayModeOption | undefined]
}>()

const { t } = useI18n()
</script>
