<template>
  <UCard :class="{ 'opacity-50': !rule.enabled }">
    <UCollapsible
      :open="expanded"
      @update:open="(val: boolean) => emit('update:expanded', val)"
    >
      <!-- Always-visible: badges + source/target -->
      <div>
        <!-- Toggle area: clicks on header or body bubble to the
             outer wrapper, which Nuxt UI binds as the CollapsibleTrigger
             (default slot is wrapped in `<CollapsibleTrigger as-child>`),
             so the accordion toggles automatically. The action footer
             below stops propagation so its buttons don't toggle. -->
        <div class="cursor-pointer">
          <!-- Header: badges + expand toggle -->
          <div class="flex items-center gap-2 mb-3">
            <UBadge
              :color="state.badgeColor(rule)"
              variant="subtle"
              size="sm"
              :title="state.badgeTitle(rule)"
            >
              <UIcon
                v-if="!rule.enabled"
                name="i-lucide-pause"
                class="w-3 h-3"
              />
              {{ state.statusLabel(rule) }}
            </UBadge>
            <UBadge variant="subtle" color="neutral" size="sm">
              {{ rule.direction === 'two_way' ? t('direction.twoWay') : t('direction.oneWay') }}
            </UBadge>
            <UBadge variant="subtle" color="neutral" size="sm">
              <UIcon name="i-lucide-clock" class="w-3 h-3" />
              {{ state.formatInterval(rule.syncIntervalSeconds) }}
            </UBadge>
            <UBadge variant="subtle" color="neutral" size="sm">
              <UIcon name="i-lucide-trash-2" class="w-3 h-3" />
              {{ state.formatDeleteMode(rule.deleteMode) }}
            </UBadge>
            <UBadge
              v-if="state.connectionBadge(rule)"
              :color="state.connectionBadge(rule)!.color"
              variant="subtle"
              size="sm"
              :title="state.connectionBadge(rule)!.title"
            >
              <UIcon :name="state.connectionBadge(rule)!.icon" class="w-3 h-3" />
              {{ state.connectionBadge(rule)!.label }}
            </UBadge>
            <UIcon
              name="i-lucide-chevron-down"
              class="w-4 h-4 text-muted ml-auto shrink-0 transition-transform duration-200"
              :class="{ 'rotate-180': expanded }"
            />
          </div>

          <!-- Body: source → target -->
          <div class="flex items-center gap-3">
            <!-- Source -->
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-1">
                <UIcon :name="state.providerIcon(rule.sourceType)" class="w-4 h-4 text-muted shrink-0" />
                <span class="text-xs text-muted">{{ t('label.source') }}</span>
                <UIcon
                  v-if="syncStore.unavailableSides.get(rule.id) === 'source'"
                  name="i-lucide-cloud-off"
                  class="w-4 h-4 text-warning shrink-0"
                  :title="t('label.unavailable')"
                />
              </div>
              <p class="text-sm font-medium truncate">
                {{ state.formatProviderLabel(rule.sourceType, rule.sourceConfig) }}
              </p>
              <p v-if="state.resolveDeviceName(rule.sourceType, rule.sourceConfig)" class="text-xs text-muted truncate">
                {{ state.resolveDeviceName(rule.sourceType, rule.sourceConfig) }}
              </p>
            </div>

            <!-- Arrow -->
            <UIcon
              :name="rule.direction === 'two_way' ? 'i-lucide-arrow-left-right' : 'i-lucide-arrow-right'"
              class="w-5 h-5 text-primary shrink-0"
            />

            <!-- Target -->
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-1">
                <UIcon :name="state.providerIcon(rule.targetType)" class="w-4 h-4 text-muted shrink-0" />
                <span class="text-xs text-muted">{{ t('label.target') }}</span>
                <UIcon
                  v-if="syncStore.unavailableSides.get(rule.id) === 'target'"
                  name="i-lucide-cloud-off"
                  class="w-4 h-4 text-warning shrink-0"
                  :title="t('label.unavailable')"
                />
              </div>
              <p class="text-sm font-medium truncate">
                {{ state.formatProviderLabel(rule.targetType, rule.targetConfig) }}
              </p>
              <p v-if="state.resolveDeviceName(rule.targetType, rule.targetConfig)" class="text-xs text-muted truncate">
                {{ state.resolveDeviceName(rule.targetType, rule.targetConfig) }}
              </p>
            </div>
          </div>
        </div>

        <!-- Footer: actions (outside toggle area; clicks here must not
             expand/collapse the card) -->
        <div
          class="flex items-center justify-end gap-1 mt-3 pt-3 border-t border-default"
          @click.stop
        >
          <UiButton
            icon="i-lucide-refresh-cw"
            variant="ghost"
            color="neutral"
            :loading="isSyncing"
            @click="emit('syncNow')"
          />
          <UChip
            :show="syncStore.getRuleLog(rule.id).length > 0"
            :text="syncStore.getRuleLog(rule.id).length"
            :color="state.hasErrorInLog(rule.id) ? 'error' : 'primary'"
            size="sm"
          >
            <UiButton
              icon="i-lucide-scroll-text"
              variant="ghost"
              :color="state.hasErrorInLog(rule.id) ? 'error' : 'neutral'"
              :title="t('actions.viewLog')"
              @click="emit('update:expanded', true)"
            />
          </UChip>
          <UiButton
            icon="i-lucide-pencil"
            variant="ghost"
            color="neutral"
            @click="emit('edit')"
          />
          <USwitch
            :model-value="rule.enabled"
            @update:model-value="(val: boolean) => emit('toggle', val)"
          />
          <UiButton
            icon="i-lucide-trash-2"
            variant="ghost"
            color="error"
            @click="emit('delete')"
          />
        </div>
      </div>

      <!-- Collapsible: progress + last result -->
      <template #content>
        <div class="mt-3 pt-3 border-t border-default">
          <!-- Active sync progress (when running).
               :key changes when a cycle restart is detected (filesDone
               regresses), forcing remount so the bar doesn't animate
               backwards from the old high to the new low. -->
          <SyncRuleProgress
            v-if="syncStore.getRuleProgress(rule.id)"
            :key="`progress-${rule.id}-${cycleKey}`"
            :progress="syncStore.getRuleProgress(rule.id)!"
            :stable-active-files="state.stableActiveFiles(rule.id)"
          />
          <!-- No active progress yet — render either the last result or
               the noData hint. The activity-log section is always shown
               below regardless of which branch above hits. -->
          <div v-else-if="!syncStore.getLastResult(rule.id)" class="text-xs text-muted">
            {{ t('progress.noData') }}
          </div>

          <!-- Last sync result + activity log (lastResult is rendered only
               when there is no active progress). -->
          <SyncRuleActivityLog
            :rule="rule"
            :last-result="syncStore.getRuleProgress(rule.id) ? null : syncStore.getLastResult(rule.id)"
            :log="syncStore.getRuleLog(rule.id)"
            :show-all-devices="showAllDevices"
            @toggle-all-devices="(val: boolean) => emit('toggleAllDevices', val)"
            @clear-log="syncStore.clearRuleLog(rule.id)"
          />
        </div>
      </template>
    </UCollapsible>
  </UCard>
</template>

<script setup lang="ts">
import type { SelectHaexSyncRules } from '~/database/schemas'
import { useSyncRulesStateInject } from '@/composables/useSyncRulesState'
import SyncRuleProgress from './SyncRuleProgress.vue'
import SyncRuleActivityLog from './SyncRuleActivityLog.vue'

defineProps<{
  rule: SelectHaexSyncRules
  isSyncing: boolean
  cycleKey: number
  expanded: boolean
  showAllDevices: boolean
}>()

const emit = defineEmits<{
  'update:expanded': [val: boolean]
  edit: []
  delete: []
  syncNow: []
  toggle: [enabled: boolean]
  toggleAllDevices: [val: boolean]
}>()

const { t } = useI18n()
const syncStore = useFileSyncStore()
const state = useSyncRulesStateInject()
</script>
