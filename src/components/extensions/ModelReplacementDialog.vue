<script setup lang="ts">
/**
 * The question of a model job whose model could not take a step (spec 055): the model failed while
 * the job ran, or it was not asked at all because the default model cannot take the step or is not
 * available. It names the model and why, and asks which other model should take the step, with a
 * tick to make that model the extension's default. holzi never picks another model itself. The job
 * waits; "Abbrechen" ends the step with the failure of the first model. A model outside this device that the user has not agreed to yet says where the job would
 * go, and choosing it agrees to that. Mounted once in the desktop, like the permission questions.
 */
import { toast } from 'vue-sonner'
import { startingEffort } from '~/lib/extensions/modelGrants'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useExtensionModelReplacementStore()

const modelId = ref('')
const effort = ref('none')
const makeDefault = ref(false)

// The model was not asked: it cannot take the step or is not available (any more).
const beforeStart = computed(
  () =>
    store.shown?.outcome === 'cannot_take' ||
    store.shown?.outcome === 'not_available',
)

const text = computed(() => {
  const shown = store.shown
  if (!shown) return ''
  const keys = {
    default: {
      failed: 'extensions.modelReplacement.defaultFailed',
      blocked: 'extensions.modelReplacement.defaultBlocked',
    },
    other: {
      failed: 'extensions.modelReplacement.failed',
      blocked: 'extensions.modelReplacement.blocked',
    },
  }
  const key =
    keys[shown.failedWasDefault ? 'default' : 'other'][
      beforeStart.value ? 'blocked' : 'failed'
    ]
  return t(key, {
    // A model the vault does not have any more has no name to show.
    model: shown.failedModel || t('extensions.modelReplacement.unknownModel'),
    why: t(`extensions.modelReplacement.outcome.${shown.outcome}`),
  })
})

const chosen = computed(() =>
  store.shown?.models.find((model) => model.id === modelId.value),
)

watch(
  () => store.shown?.requestId,
  () => {
    modelId.value = store.shown?.models[0]?.id ?? ''
    makeDefault.value = false
  },
  { immediate: true },
)
// Another model offers other efforts.
watch(chosen, (model) => {
  effort.value = startingEffort(model?.efforts ?? [])
})

async function answerAsync(use: boolean) {
  const question = store.shown
  if (!question) return
  try {
    await store.answerAsync(
      question.requestId,
      use
        ? {
            modelId: modelId.value,
            effort: effort.value,
            makeDefault: makeDefault.value,
          }
        : null,
    )
  } catch (error) {
    toast.error(errString(error))
  }
}
</script>

<template>
  <UiDrawerModal
    v-if="store.shown"
    :open="true"
    :title="
      t('extensions.modelReplacement.title', {
        name: store.shown.displayName,
      })
    "
    :dismissible="false"
  >
    <template #content>
      <div class="space-y-3" data-testid="extension-model-replacement">
        <p class="text-sm">
          {{ text }}
        </p>
        <p
          v-if="store.shown.reason"
          class="text-xs text-muted-foreground"
          data-testid="extension-model-replacement-reason"
        >
          {{ store.shown.reason }}
        </p>
        <ExtensionsModelPick
          v-model:model-id="modelId"
          v-model:effort="effort"
          :models="store.shown.models"
        />
        <ExtensionsModelDestination
          v-if="chosen?.needsConsent"
          :destination="{
            provider: chosen.provider,
            host: chosen.host,
            place: chosen.place,
          }"
          :transfer="store.shown.transfer"
          data-testid="extension-model-replacement-consent"
        />
        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="makeDefault"
            data-testid="extension-model-replacement-make-default"
          />
          {{ t('extensions.permissionRequest.model.makeDefault') }}
        </label>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          size="sm"
          variant="outline"
          data-testid="extension-model-replacement-cancel"
          @click="answerAsync(false)"
        >
          {{ t('extensions.modelReplacement.cancel') }}
        </UiButton>
        <UiButton
          size="sm"
          :disabled="!chosen"
          data-testid="extension-model-replacement-use"
          @click="answerAsync(true)"
        >
          {{ t('extensions.modelReplacement.use') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
