<script setup lang="ts">
/**
 * The exclusions of a rule (spec 061 US4 scenario 8, FR-038, FR-041, FR-042): the `.haex-ignore`
 * file of place A in a small editor, the fixed exclusions that always hold, how many files and
 * folders both places leave out now, and lines holzi does not understand. The file is the only
 * copy of the patterns; a place A that cannot be written shows them with the reason.
 */
import type { SyncRuleIgnore } from '@bindings/SyncRuleIgnore'
import type { SyncRuleView } from '@bindings/SyncRuleView'
import { asSyncRuleError } from '~/composables/useSyncRules'

const props = defineProps<{
  view: SyncRuleView | null
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const { ignoreGetAsync, ignoreSetAsync } = useSyncRules()

const shown = ref<SyncRuleIgnore | null>(null)
const content = ref('')
const loading = ref(false)
const saving = ref(false)
const saved = ref(false)
const error = ref<string | null>(null)

function showError(cause: unknown) {
  const code = asSyncRuleError(cause)?.code
  error.value = code
    ? t(`files.syncRules.error.${code}`)
    : t('files.syncRules.failed')
}

function take(next: SyncRuleIgnore) {
  shown.value = next
  content.value = next.content
}

watch(open, async (isOpen) => {
  const view = props.view
  if (!isOpen || !view) return
  shown.value = null
  content.value = ''
  saved.value = false
  error.value = null
  loading.value = true
  try {
    take(await ignoreGetAsync(view.rule.id))
  } catch (cause) {
    showError(cause)
  } finally {
    loading.value = false
  }
})

watch(content, () => {
  saved.value = false
})

async function saveAsync() {
  const view = props.view
  if (!view || saving.value || !shown.value?.writable) return
  saving.value = true
  try {
    take(await ignoreSetAsync(view.rule.id, content.value))
    error.value = null
    saved.value = true
  } catch (cause) {
    showError(cause)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('files.syncRules.ignore.title', { name: view?.rule.name ?? '' })"
  >
    <template #content>
      <form
        class="flex flex-col gap-3"
        data-testid="files-sync-rule-ignore"
        @submit.prevent="saveAsync"
      >
        <p v-if="loading" class="text-sm text-muted-foreground">
          {{ t('files.syncRules.ignore.loading') }}
        </p>
        <template v-if="shown">
          <p class="text-xs text-muted-foreground">
            {{ t('files.syncRules.ignore.hint') }}
          </p>
          <UiTextarea
            id="files-sync-rule-ignore-text"
            v-model="content"
            :label="t('files.syncRules.ignore.label')"
            label-bg="var(--background)"
            rows="8"
            class="font-mono"
            :disabled="!shown.writable"
            data-testid="files-sync-rule-ignore-text"
          />
          <p
            v-if="!shown.writable"
            class="text-xs text-warning"
            data-testid="files-sync-rule-ignore-read-only"
          >
            {{
              t('files.syncRules.ignore.readOnly', {
                reason: shown.reason ?? '',
              })
            }}
          </p>
          <ul
            v-if="shown.lineErrors.length"
            class="flex flex-col gap-0.5"
            data-testid="files-sync-rule-ignore-line-errors"
          >
            <li
              v-for="line in shown.lineErrors"
              :key="line.line"
              class="text-xs text-destructive"
            >
              {{
                t('files.syncRules.ignore.lineError', {
                  line: line.line,
                  text: line.text,
                })
              }}
            </li>
          </ul>
          <p
            class="text-xs text-muted-foreground"
            data-testid="files-sync-rule-ignore-count"
          >
            {{ t('files.syncRules.ignore.ignoredNow', shown.ignoredNow) }}
          </p>
          <div class="flex flex-col gap-1">
            <span class="text-sm font-medium">{{
              t('files.syncRules.ignore.fixed')
            }}</span>
            <ul class="flex flex-wrap gap-1">
              <li
                v-for="line in shown.fixed"
                :key="line"
                class="rounded bg-muted px-1.5 py-0.5 font-mono text-xs"
              >
                {{ line }}
              </li>
            </ul>
          </div>
        </template>
        <p v-if="error" class="text-sm text-destructive" role="alert">
          {{ error }}
        </p>
        <p
          v-if="saved"
          class="text-xs text-muted-foreground"
          data-testid="files-sync-rule-ignore-saved"
        >
          {{ t('files.syncRules.ignore.saved') }}
        </p>
        <div class="flex justify-end gap-2">
          <UiButton type="button" variant="outline" @click="open = false">{{
            t('files.cancel')
          }}</UiButton>
          <UiButton
            type="submit"
            :disabled="!shown?.writable"
            :loading="saving"
            data-testid="files-sync-rule-ignore-save"
            >{{ t('files.syncRules.ignore.save') }}</UiButton
          >
        </div>
      </form>
    </template>
  </UiDrawerModal>
</template>
