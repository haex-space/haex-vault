<template>
  <!-- Step 1: Select file or paste JSON -->
  <template v-if="!importParsed">
    <div class="space-y-4 mt-4">
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-file-up"
        block
        @click="onSelectImportFileAsync"
      >
        {{ t('file.selectFile') }}
      </UButton>

      <USeparator :label="t('file.orPaste')" />

      <UiTextarea
        v-model="importJsonProxy"
        :label="t('file.jsonLabel')"
        :placeholder="t('file.jsonPlaceholder')"
        :rows="6"
        data-testid="contacts-import-json"
      />
    </div>
  </template>

  <!-- Step 2: Preview & select -->
  <template v-else>
    <div class="space-y-4 mt-4">
      <div class="flex items-center gap-3 p-3 rounded-lg border border-default">
        <UiAvatar
          v-if="importParsed.avatar"
          :src="importParsed.avatar"
          :seed="importParsed.did"
          avatar-style="toon-head"
          size="sm"
        />
        <div class="min-w-0 flex-1">
          <p class="font-medium truncate">{{ importParsed.name || importParsed.did.slice(0, 20) + '...' }}</p>
          <p class="text-xs text-muted truncate">{{ importParsed.did }}</p>
        </div>
      </div>

      <div
        v-if="importParsed.avatar"
        class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
      >
        <UCheckbox v-model="importIncludeAvatarProxy" />
        <UiAvatar
          :src="importParsed.avatar"
          :seed="importParsed.did"
          avatar-style="toon-head"
          size="sm"
        />
        <span class="text-sm">{{ t('file.includeAvatar') }}</span>
      </div>

      <div
        v-if="importParsed.claims.length"
        class="space-y-2"
      >
        <span class="text-sm font-medium">{{ t('file.selectClaims') }}</span>
        <div
          v-for="(claim, index) in importParsed.claims"
          :key="index"
          class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
        >
          <UCheckbox
            :model-value="importSelectedClaimIndices.has(index)"
            @update:model-value="toggleImportClaim(index)"
          />
          <div class="min-w-0 flex-1">
            <span class="text-xs font-medium text-muted">{{ claim.type }}</span>
            <p class="text-sm truncate">{{ claim.value }}</p>
          </div>
        </div>
      </div>
    </div>
  </template>
</template>

<script setup lang="ts">
import type { ImportParsed } from '@/composables/contacts/useAddContactWizard'

const props = defineProps<{
  importJson: string
  importParsed: ImportParsed | null
  importSelectedClaimIndices: Set<number>
  importIncludeAvatar: boolean
}>()

const emit = defineEmits<{
  'update:importJson': [value: string]
  'update:importIncludeAvatar': [value: boolean]
  selectFile: []
  toggleClaim: [index: number]
}>()

const { t } = useI18n()

const importJsonProxy = computed({
  get: () => props.importJson,
  set: v => emit('update:importJson', v),
})

const importIncludeAvatarProxy = computed({
  get: () => props.importIncludeAvatar,
  set: v => emit('update:importIncludeAvatar', v),
})

const onSelectImportFileAsync = () => emit('selectFile')
const toggleImportClaim = (index: number) => emit('toggleClaim', index)
</script>
