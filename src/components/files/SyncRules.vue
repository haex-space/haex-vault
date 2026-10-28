<script setup lang="ts">
/**
 * "Sync-Regeln" (spec 061 US1, US4, FR-025, FR-032): the rules of this device with their places,
 * direction, status and last run; run now, pause or resume, delete, and the files the last run
 * could not handle. Deleting a rule leaves the files of both places alone (FR-006).
 */
import type { Sources } from '@bindings/Sources'
import type { SyncRulePlace } from '@bindings/SyncRulePlace'
import type { SyncRuleView } from '@bindings/SyncRuleView'
import { asSyncRuleError } from '~/composables/useSyncRules'
import { STATE_ICONS } from '~/lib/files/syncRules'
import { useSyncRulesStore } from '~/stores/syncRules'

const props = defineProps<{
  sources: Sources | null
}>()

const open = defineModel<boolean>('open', { required: true })

const { t, d } = useI18n()
const store = useSyncRulesStore()
const { setEnabledAsync, deleteAsync, runNowAsync } = useSyncRules()

const creating = ref(false)
const busy = ref<string | null>(null)
const deleting = ref<SyncRuleView | null>(null)

watch(open, (isOpen) => {
  if (!isOpen) return
  store.initialize()
  void store.startListening()
  void store.loadAsync()
})

/** The name a place shows: a storage or device by its name, the device by its path. */
function placeLabel(place: SyncRulePlace): string {
  if (place.source.kind === 'device') return place.path
  const id = place.source.storageId
  const named =
    props.sources?.storages.find((storage) => storage.id === id)?.name ??
    props.sources?.devices.find((device) => device.id === id)?.name ??
    t('files.syncRules.unknownSource')
  return place.path === '/' ? named : `${named} ${place.path}`
}

function lastRun(view: SyncRuleView): string {
  const at = view.rule.lastRunAt
  return at
    ? t('files.syncRules.lastRun', {
        at: d(new Date(at), { dateStyle: 'short', timeStyle: 'short' }),
      })
    : t('files.syncRules.neverRun')
}

async function act(view: SyncRuleView, work: () => Promise<unknown>) {
  if (busy.value) return
  busy.value = view.rule.id
  try {
    await work()
    await store.loadAsync()
  } catch (cause) {
    const code = asSyncRuleError(cause)?.code
    store.error = code
      ? t(`files.syncRules.error.${code}`)
      : t('files.syncRules.failed')
  } finally {
    busy.value = null
  }
}

async function confirmDeleteAsync() {
  const view = deleting.value
  deleting.value = null
  // action-exempt: sync-rule controls are window-local; no sync-rule action catalog exists yet.
  if (view) await act(view, () => deleteAsync(view.rule.id))
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('files.syncRules.title')">
    <template #content>
      <div class="flex flex-col gap-3" data-testid="files-sync-rules">
        <div class="flex justify-end">
          <UiButton
            type="button"
            size="sm"
            data-testid="files-sync-rules-new"
            @click="creating = true"
          >
            <Icon name="lucide:plus" class="size-4" />
            {{ t('files.syncRules.create.open') }}
          </UiButton>
        </div>
        <p
          v-if="store.error"
          class="text-sm text-destructive"
          role="alert"
          data-testid="files-sync-rules-error"
        >
          {{ store.error }}
        </p>
        <p
          v-if="!store.loading && store.rules.length === 0"
          class="text-sm text-muted-foreground"
        >
          {{ t('files.syncRules.empty') }}
        </p>
        <ul class="flex flex-col gap-3">
          <li
            v-for="view in store.rules"
            :key="view.rule.id"
            class="flex flex-col gap-2 rounded-lg border p-3"
            :data-testid="`files-sync-rule-${view.rule.name}`"
          >
            <div class="flex items-start gap-2">
              <Icon
                :name="STATE_ICONS[view.status.state]"
                class="mt-0.5 size-4 shrink-0 text-muted-foreground"
              />
              <div class="flex min-w-0 flex-1 flex-col">
                <span class="truncate font-medium">{{ view.rule.name }}</span>
                <span class="truncate text-xs text-muted-foreground">
                  {{ placeLabel(view.rule.placeA) }}
                  {{ view.rule.direction === 'both' ? '↔' : '→' }}
                  {{ placeLabel(view.rule.placeB) }}
                </span>
                <span class="text-xs text-muted-foreground">
                  <ShadcnBadge
                    variant="secondary"
                    :data-testid="`files-sync-rule-state-${view.rule.name}`"
                    >{{
                      t(`files.syncRules.state.${view.status.state}`)
                    }}</ShadcnBadge
                  >
                  {{ lastRun(view) }}
                </span>
                <span
                  v-if="
                    view.status.reason &&
                    (view.status.state === 'waiting' ||
                      view.status.state === 'error')
                  "
                  class="text-xs text-warning"
                  >{{ view.status.reason }}</span
                >
              </div>
              <UiButton
                type="button"
                variant="ghost"
                size="icon-sm"
                :disabled="busy !== null || !view.rule.enabled"
                :aria-label="t('files.syncRules.runNow')"
                :tooltip="t('files.syncRules.runNow')"
                :data-testid="`files-sync-rule-run-${view.rule.name}`"
                @click="act(view, () => runNowAsync(view.rule.id))"
              >
                <Icon name="lucide:refresh-cw" class="size-4" />
              </UiButton>
              <UiButton
                type="button"
                variant="ghost"
                size="icon-sm"
                :disabled="busy !== null"
                :aria-label="
                  view.rule.enabled
                    ? t('files.syncRules.pause')
                    : t('files.syncRules.resume')
                "
                :tooltip="
                  view.rule.enabled
                    ? t('files.syncRules.pause')
                    : t('files.syncRules.resume')
                "
                :data-testid="`files-sync-rule-toggle-${view.rule.name}`"
                @click="
                  act(view, () =>
                    setEnabledAsync(view.rule.id, !view.rule.enabled),
                  )
                "
              >
                <Icon
                  :name="view.rule.enabled ? 'lucide:pause' : 'lucide:play'"
                  class="size-4"
                />
              </UiButton>
              <UiButton
                type="button"
                variant="ghost"
                size="icon-sm"
                :disabled="busy !== null"
                :aria-label="t('files.syncRules.delete')"
                :tooltip="t('files.syncRules.delete')"
                :data-testid="`files-sync-rule-delete-${view.rule.name}`"
                @click="deleting = view"
              >
                <Icon name="lucide:trash-2" class="size-4" />
              </UiButton>
            </div>
            <p
              v-if="view.status.deviations > 0"
              class="text-xs text-muted-foreground"
            >
              {{
                t('files.syncRules.deviations', {
                  count: view.status.deviations,
                })
              }}
            </p>
            <ul v-if="view.status.errors.length" class="flex flex-col gap-1">
              <li
                v-for="failure in view.status.errors"
                :key="failure.path"
                class="text-xs text-destructive"
              >
                {{ failure.path }}: {{ failure.reason }}
              </li>
            </ul>
          </li>
        </ul>
      </div>
    </template>
  </UiDrawerModal>
  <FilesSyncRuleDialog
    v-model:open="creating"
    :sources="sources"
    @saved="store.loadAsync()"
  />
  <ShadcnAlertDialog
    :open="deleting !== null"
    @update:open="(value: boolean) => (deleting = value ? deleting : null)"
  >
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('files.syncRules.deleteTitle')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>{{
          t('files.syncRules.deleteText', { name: deleting?.rule.name ?? '' })
        }}</ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('files.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          data-testid="files-sync-rule-delete-confirm"
          @click="confirmDeleteAsync"
        >
          {{ t('files.syncRules.delete') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
