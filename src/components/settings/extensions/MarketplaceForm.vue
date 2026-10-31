<script setup lang="ts">
/**
 * Adding or changing a marketplace (spec 069 US4, FR-021, FR-024): `/extensions/marketplaces/new`
 * or `…/:id`. "Testen und speichern" tries the address (`GET /health` and the first page), saves
 * when it answers, and when it does not shows why and still lets the user save: a marketplace may
 * be down for a while, or on a network this device is not on. A deliberate button, no saving on
 * input (FR-021). The address is shown the way it will be stored; `http://` carries a hint.
 *
 * A marketplace may ask for a login (spec 069 US5): none, a bearer token or a user name and
 * password. The login is typed here and becomes an entry of the password manager, or it is an
 * entry the user chooses; the table keeps only the id of that entry (FR-026). With `http://` the
 * form refuses a login (FR-027).
 */
import { invoke } from '@tauri-apps/api/core'
import type { CredentialsEntry } from '@bindings/CredentialsEntry'
import type { MarketplaceAuthType } from '@bindings/MarketplaceAuthType'
import type { MarketplaceInput } from '@bindings/MarketplaceInput'
import type { MarketplaceTestResult } from '@bindings/MarketplaceTestResult'
import type { MarketplaceView } from '@bindings/MarketplaceView'
import {
  clearLoginSecrets,
  emptyLogin,
  loginFields,
  loginProblem,
  needsLoginInput,
  type KeptLogin,
} from '~/lib/marketplace/loginForm'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import CredentialsEntryPicker from '~/components/settings/storage/CredentialsEntryPicker.vue'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const { openApp } = useWmTab()

const NEW = 'new'
const id = computed(() => router.route.params.id ?? NEW)
const isNew = computed(() => id.value === NEW)
/** The public marketplace is only switched off, never deleted. */
const deletable = ref(false)

const name = ref('')
const baseUrl = ref('')
const loaded = ref(false)
const missing = ref(false)
const busy = ref(false)
const result = ref<MarketplaceTestResult | null>(null)
const failure = ref<string | null>(null)
const deleting = ref(false)
/** The login of the saved row; the form keeps it unless the user asks for another. */
const kept = ref<KeptLogin | null>(null)
const keptEntry = ref<CredentialsEntry | null>(null)
const login = reactive(emptyLogin())
let loadSequence = 0

const authOptions = computed<SettingsSelectOption[]>(() =>
  (['none', 'bearer', 'basic'] as const).map((type) => ({
    value: type,
    label: t(`settings.extensions.marketplaces.form.auth.${type}`),
  })),
)
const sourceOptions = computed<SettingsSelectOption[]>(() =>
  (['new', 'entry'] as const).map((source) => ({
    value: source,
    label: t(`settings.storage.credentialsSource.${source}`),
  })),
)

function resetLogin(authType: MarketplaceAuthType = 'none') {
  Object.assign(login, emptyLogin(), { authType })
}

async function loadAsync() {
  const mine = ++loadSequence
  loaded.value = false
  missing.value = false
  result.value = null
  failure.value = null
  if (isNew.value) {
    name.value = ''
    baseUrl.value = ''
    kept.value = null
    keptEntry.value = null
    resetLogin()
    loaded.value = true
    return
  }
  try {
    const all = await invoke<MarketplaceView[]>('marketplace_list')
    if (mine !== loadSequence) return
    const row = all.find((market) => market.id === id.value)
    if (!row) {
      missing.value = true
    } else {
      name.value = row.name
      baseUrl.value = row.baseUrl
      kept.value = {
        authType: row.authType,
        credentialsItemId: row.credentialsItemId,
      }
      keptEntry.value = row.credentialsEntry ?? null
      deletable.value = row.deletable
      resetLogin(row.authType)
      // A login that is gone or lacks its fields is asked for again.
      login.replace = row.credentials === 'missing'
    }
  } catch (error) {
    if (mine !== loadSequence) return
    failure.value = errString(error)
  }
  loaded.value = true
}

watch(id, loadAsync, { immediate: true })

watch([name, baseUrl, () => ({ ...login })], () => {
  result.value = null
})

const input = computed<MarketplaceInput>(() => ({
  id: isNew.value ? undefined : id.value,
  name: name.value,
  baseUrl: baseUrl.value,
  ...loginFields(login, kept.value),
}))
const usesHttp = computed(() => /^http:\/\//i.test(baseUrl.value.trim()))
const problem = computed(() => loginProblem(login, baseUrl.value, kept.value))
const asksForLogin = computed(() => needsLoginInput(login, kept.value))
const hasKeptLogin = computed(
  () =>
    login.authType !== 'none' &&
    kept.value?.authType === login.authType &&
    !!kept.value.credentialsItemId,
)
const complete = computed(
  () =>
    name.value.trim() !== '' && baseUrl.value.trim() !== '' && !problem.value,
)
const pickerSource = computed(() => ({
  command: 'marketplace_credentials_candidates',
  args: { authType: login.authType },
}))

function backToList() {
  router.replace('/extensions/marketplaces')
}

async function saveAsync() {
  busy.value = true
  failure.value = null
  try {
    await invoke<MarketplaceView>('marketplace_save', { input: input.value })
    clearLoginSecrets(login)
    backToList()
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}

/** Tries the address; a marketplace that answers is saved at once, one that does not is shown
 * with its cause and can still be saved. */
async function testAndSaveAsync() {
  if (!complete.value || busy.value) return
  busy.value = true
  failure.value = null
  result.value = null
  try {
    result.value = await invoke<MarketplaceTestResult>('marketplace_test', {
      input: input.value,
    })
  } catch (error) {
    failure.value = errString(error)
    busy.value = false
    return
  }
  busy.value = false
  if (result.value.ok) await saveAsync()
}

// action-exempt: marketplace sources affect extension-install trust; only the user-confirmed form may remove one.
async function deleteAsync() {
  busy.value = true
  try {
    await invoke('marketplace_delete', { id: id.value })
    deleting.value = false
    backToList()
  } catch (error) {
    failure.value = errString(error)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-3" data-testid="settings-marketplace-form">
    <p v-if="missing" class="px-1 text-sm text-muted-foreground" role="status">
      {{ t('settings.extensions.marketplaces.form.notFound') }}
    </p>

    <form
      v-else-if="loaded"
      class="flex flex-col gap-3"
      @submit.prevent="testAndSaveAsync"
    >
      <SettingsGroup>
        <li class="px-4 py-3">
          <UiInput
            id="settings-marketplace-name"
            v-model="name"
            :label="t('settings.extensions.marketplaces.form.name')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            maxlength="80"
            autocomplete="off"
            data-testid="settings-marketplace-name"
          />
        </li>
        <li class="flex flex-col gap-2 px-4 py-3">
          <UiInput
            id="settings-marketplace-url"
            v-model="baseUrl"
            :label="t('settings.extensions.marketplaces.form.url')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="url"
            autocomplete="off"
            data-testid="settings-marketplace-url"
          />
          <p class="text-sm text-muted-foreground">
            {{ t('settings.extensions.marketplaces.form.urlHint') }}
          </p>
          <p
            v-if="usesHttp"
            class="text-sm text-destructive"
            data-testid="settings-marketplace-http-hint"
          >
            {{ t('settings.extensions.marketplaces.form.httpHint') }}
          </p>
        </li>
        <SettingsRow
          :title="t('settings.extensions.marketplaces.form.auth.label')"
          label-for="settings-marketplace-auth"
        >
          <SettingsSelect
            id="settings-marketplace-auth"
            v-model="login.authType"
            :options="authOptions"
            data-testid="settings-marketplace-auth"
          />
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup
        v-if="login.authType !== 'none'"
        :label="t('settings.extensions.marketplaces.form.login.group')"
      >
        <li
          v-if="problem === 'insecure'"
          class="px-4 py-3 text-sm text-destructive"
          role="alert"
          data-testid="settings-marketplace-insecure"
        >
          {{ t('settings.extensions.marketplaces.form.login.insecure') }}
        </li>
        <li
          v-if="kept?.credentialsItemId && !keptEntry && !asksForLogin"
          class="px-4 py-3 text-sm text-muted-foreground"
        >
          {{ t('settings.extensions.marketplaces.form.login.notVisible') }}
        </li>
        <SettingsRow
          v-if="hasKeptLogin && keptEntry"
          :title="t('settings.storage.credentialsLinked')"
          :description="keptEntry.title || t('passwords.untitled')"
          data-testid="settings-marketplace-login-entry"
        >
          <UiButton
            type="button"
            size="sm"
            variant="outline"
            data-testid="settings-marketplace-open-login-entry"
            @click="openApp('system.passwords', `/entry/${keptEntry.id}`)"
          >
            {{ t('settings.storage.openCredentialsEntry') }}
          </UiButton>
        </SettingsRow>
        <SettingsOptionRow
          v-if="hasKeptLogin"
          type="checkbox"
          :checked="login.replace"
          :title="t('settings.extensions.marketplaces.form.login.replace')"
          data-testid="settings-marketplace-replace-login"
          @change="login.replace = $event"
        />
        <template v-if="asksForLogin">
          <SettingsRow
            :title="t('settings.storage.credentialsSource.label')"
            label-for="settings-marketplace-login-source"
          >
            <SettingsSelect
              id="settings-marketplace-login-source"
              v-model="login.source"
              :options="sourceOptions"
              data-testid="settings-marketplace-login-source"
            />
          </SettingsRow>
          <template v-if="login.source === 'new'">
            <li v-if="login.authType === 'basic'" class="px-4 py-3">
              <UiInput
                id="settings-marketplace-username"
                v-model="login.username"
                :label="
                  t('settings.extensions.marketplaces.form.login.username')
                "
                :labels="fieldLabels.input.value"
                label-bg="var(--muted)"
                autocomplete="off"
                data-testid="settings-marketplace-username"
              />
            </li>
            <li class="px-4 py-3">
              <UiInputPassword
                id="settings-marketplace-secret"
                v-model="login.secret"
                :label="
                  t(
                    login.authType === 'basic'
                      ? 'settings.extensions.marketplaces.form.login.password'
                      : 'settings.extensions.marketplaces.form.login.token',
                  )
                "
                :labels="fieldLabels.password.value"
                label-bg="var(--muted)"
                autocomplete="new-password"
                data-testid="settings-marketplace-secret"
              />
            </li>
          </template>
        </template>
      </SettingsGroup>
      <CredentialsEntryPicker
        v-if="
          login.authType !== 'none' && asksForLogin && login.source === 'entry'
        "
        v-model="login.entryId"
        :source="pickerSource"
        label-key="settings.extensions.marketplaces.form.login.entryLabel"
        hint-key="settings.extensions.marketplaces.form.login.onlyFitting"
      />

      <p
        v-if="result"
        :class="[
          'rounded-xl px-4 py-3 text-sm',
          result.ok ? 'bg-muted' : 'bg-destructive/10 text-destructive',
        ]"
        role="status"
        data-testid="settings-marketplace-test-result"
      >
        {{
          result.ok
            ? t('settings.extensions.marketplaces.test.ok', {
                count: result.count ?? 0,
              })
            : t('settings.extensions.marketplaces.test.failed', {
                cause: t(
                  `settings.extensions.marketplaces.testError.${result.error}`,
                ),
              })
        }}
      </p>
      <p v-if="failure" class="text-sm text-destructive" role="alert">
        {{ failure }}
      </p>

      <div class="flex flex-wrap justify-end gap-2">
        <UiButton
          v-if="!isNew && deletable"
          type="button"
          variant="destructive"
          :disabled="busy"
          data-testid="settings-marketplace-delete"
          @click="deleting = true"
        >
          {{ t('settings.extensions.marketplaces.form.delete') }}
        </UiButton>
        <UiButton
          v-if="result && !result.ok"
          type="button"
          variant="outline"
          :disabled="busy || !complete"
          data-testid="settings-marketplace-save-anyway"
          @click="saveAsync"
        >
          {{ t('settings.extensions.marketplaces.form.saveAnyway') }}
        </UiButton>
        <UiButton
          type="submit"
          :loading="busy"
          :disabled="busy || !complete"
          data-testid="settings-marketplace-save"
        >
          {{ t('settings.extensions.marketplaces.form.testAndSave') }}
        </UiButton>
      </div>
    </form>

    <ShadcnAlertDialog v-model:open="deleting">
      <ShadcnAlertDialogContent>
        <ShadcnAlertDialogHeader>
          <ShadcnAlertDialogTitle>{{
            t('settings.extensions.marketplaces.form.deleteTitle', {
              name,
            })
          }}</ShadcnAlertDialogTitle>
          <ShadcnAlertDialogDescription>{{
            t('settings.extensions.marketplaces.form.deleteBody')
          }}</ShadcnAlertDialogDescription>
        </ShadcnAlertDialogHeader>
        <ShadcnAlertDialogFooter>
          <ShadcnAlertDialogCancel>{{
            t('settings.extensions.marketplaces.form.cancel')
          }}</ShadcnAlertDialogCancel>
          <UiButton
            type="button"
            variant="destructive"
            :loading="busy"
            data-testid="settings-marketplace-delete-confirm"
            @click="deleteAsync"
          >
            {{ t('settings.extensions.marketplaces.form.delete') }}
          </UiButton>
        </ShadcnAlertDialogFooter>
      </ShadcnAlertDialogContent>
    </ShadcnAlertDialog>
  </section>
</template>
