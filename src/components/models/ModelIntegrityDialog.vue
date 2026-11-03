<script setup lang="ts">
import { toast } from 'vue-sonner'

const { t } = useI18n()

/**
 * Presentational integrity decision dialog (contracts/tauri-commands.md
 * §"load_model und lokale Integritätsprüfung"). The three emitted actions
 * map 1:1 to the backend's documented options — `loadUntrusted` calls
 * `activate_local_model_with_integrity_override`, `repairSource` re-runs the
 * model's stored HF download or local import, `chooseOther` opens the
 * model picker. This component owns none of that logic; the container
 * that opens it does, since only it knows the model's stored source.
 */
const props = defineProps<{
  open: boolean
  errorKind:
    'ModelIntegrityMismatch' | 'ModelIntegrityUnknown' | 'ModelIntegrityError'
  expectedSha256: string | null
  actualSha256: string | null
  busy?: boolean
  /** Already-localized text — callers resolve their own error key via `t()`; shown as a toast. */
  actionError?: string | null
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  loadUntrusted: []
  repairSource: []
  chooseOther: []
}>()

watch(
  () => props.actionError,
  (message) => {
    if (message) toast.error(message)
  },
)

const descriptionKey = computed(() => {
  switch (props.errorKind) {
    case 'ModelIntegrityMismatch':
      return 'models.integrityDialog.mismatchDescription'
    case 'ModelIntegrityUnknown':
      return 'models.integrityDialog.unknownDescription'
    default:
      return 'models.integrityDialog.errorDescription'
  }
})
</script>

<template>
  <UiDrawerModal
    :open="open"
    :title="t('models.integrityDialog.title')"
    @update:open="emit('update:open', $event)"
  >
    <template #content>
      <div class="flex flex-col gap-3 px-6 py-2">
        <p class="text-sm">
          {{ t(descriptionKey) }}
        </p>
        <dl
          v-if="expectedSha256 || actualSha256"
          class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs"
        >
          <template v-if="expectedSha256">
            <dt class="text-muted-foreground">
              {{ t('models.integrityDialog.expected') }}
            </dt>
            <dd class="truncate font-mono">
              {{ expectedSha256 }}
            </dd>
          </template>
          <template v-if="actualSha256">
            <dt class="text-muted-foreground">
              {{ t('models.integrityDialog.actual') }}
            </dt>
            <dd class="truncate font-mono">
              {{ actualSha256 }}
            </dd>
          </template>
        </dl>
      </div>
    </template>
    <template #footer>
      <div class="flex flex-col gap-2">
        <UiButton
          type="button"
          variant="outline"
          :disabled="busy"
          @click="emit('repairSource')"
        >
          {{ t('models.integrityDialog.repairSource') }}
        </UiButton>
        <UiButton
          type="button"
          variant="outline"
          :disabled="busy"
          @click="emit('chooseOther')"
        >
          {{ t('models.integrityDialog.chooseOther') }}
        </UiButton>
        <UiButton
          type="button"
          variant="destructive"
          :loading="busy"
          @click="emit('loadUntrusted')"
        >
          {{ t('models.integrityDialog.loadUntrusted') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
