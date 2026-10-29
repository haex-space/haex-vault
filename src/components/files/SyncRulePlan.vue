<script setup lang="ts">
/**
 * The preview of a rule (spec 061 US4 scenarios 4 and 6, FR-007, FR-012a, FR-042): what a run
 * would do now, grouped by action, how many files it compares by content and how many the
 * exclusions leave out. Opening it changes nothing.
 */
import type { SyncRuleDraft } from '@bindings/SyncRuleDraft'
import type { SyncRulePlan } from '@bindings/SyncRulePlan'
import { asSyncRuleError } from '~/composables/useSyncRules'
import { planGroups } from '~/lib/files/syncRules'

const props = defineProps<{
  /** A stored rule, a new draft, or both for a change; `null` while closed. */
  target: { id?: string; draft?: SyncRuleDraft } | null
  /** The names of both places. */
  labels: { a: string; b: string }
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { previewAsync } = useSyncRules()

const plan = ref<SyncRulePlan | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

watch(open, async (isOpen) => {
  if (!isOpen || !props.target) return
  plan.value = null
  error.value = null
  loading.value = true
  try {
    plan.value = await previewAsync(props.target)
  } catch (cause) {
    const code = asSyncRuleError(cause)?.code
    error.value = code
      ? t(`files.syncRules.error.${code}`)
      : t('files.syncRules.failed')
  } finally {
    loading.value = false
  }
})

const groups = computed(() => planGroups(plan.value?.steps ?? []))
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('files.syncRules.plan.title')">
    <template #content>
      <div class="flex flex-col gap-3" data-testid="files-sync-rule-plan">
        <p v-if="loading" class="text-sm text-muted-foreground">
          {{ t('files.syncRules.plan.loading') }}
        </p>
        <p v-if="error" class="text-sm text-destructive" role="alert">
          {{ error }}
        </p>
        <template v-if="plan">
          <p
            v-if="groups.length === 0"
            class="text-sm text-muted-foreground"
            data-testid="files-sync-rule-plan-empty"
          >
            {{ t('files.syncRules.plan.nothing') }}
          </p>
          <section
            v-for="group in groups"
            :key="group.action"
            class="flex flex-col gap-1"
            :data-testid="`files-sync-rule-plan-${group.action}`"
          >
            <h3 class="text-sm font-medium">
              {{
                t(
                  `files.syncRules.plan.action.${group.action}`,
                  group.steps.length,
                )
              }}
            </h3>
            <ul class="flex flex-col gap-0.5">
              <li
                v-for="(step, index) in group.steps"
                :key="`${step.path}-${index}`"
                class="flex min-w-0 gap-2 text-xs"
              >
                <span class="min-w-0 flex-1 truncate">{{ step.path }}</span>
                <span v-if="step.detail" class="truncate text-muted-foreground">
                  {{
                    group.action === 'deviation'
                      ? t(`files.syncRules.plan.deviation.${step.detail}`)
                      : `→ ${step.detail}`
                  }}
                </span>
                <span v-if="step.side" class="shrink-0 text-muted-foreground">{{
                  labels[step.side]
                }}</span>
              </li>
            </ul>
          </section>
          <p
            v-if="plan.hashReads > 0"
            class="text-xs text-muted-foreground"
            data-testid="files-sync-rule-plan-hash-reads"
          >
            {{ t('files.syncRules.plan.hashReads', plan.hashReads) }}
          </p>
          <p
            v-if="plan.ignored > 0"
            class="text-xs text-muted-foreground"
            data-testid="files-sync-rule-plan-ignored"
          >
            {{ t('files.syncRules.plan.ignored', plan.ignored) }}
          </p>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>
