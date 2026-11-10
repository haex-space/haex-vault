<script setup lang="ts">
/**
 * One cookie of the agent browser's profile (spec 065 FR-046, research R17): its path, expiry and
 * flags; the value only after a click on "Wert zeigen"; delete, and change the value and expiry
 * (a date and time, or a cookie of the session). The window reads and writes through commands no
 * agent can call; errors are toasts. The parent reloads the list after a change or a deletion.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { CookieRef } from '~/types/bindings/CookieRef'
import type { CookieView } from '~/types/bindings/CookieView'

const props = defineProps<{ cookie: CookieView }>()
const emit = defineEmits<{ changed: [] }>()

const { t, locale } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const setCookie = useActionOrThrow('settings.agentBrowser.profile.cookie.set')
const deleteCookie = useActionOrThrow(
  'settings.agentBrowser.profile.cookie.delete',
)

/** The value, once the user asked for it; gone again when the row is closed. */
const value = ref<string | null>(null)
const editing = ref(false)
const draftValue = ref('')
const draftExpires = ref('')
const session = ref(false)
const busy = ref(false)

const ref_ = computed<CookieRef>(() => ({
  name: props.cookie.name,
  domain: props.cookie.domain,
  path: props.cookie.path,
}))

const description = computed(() => {
  const parts = [
    t('agentBrowser.settings.cookies.path', { path: props.cookie.path }),
    props.cookie.expires == null
      ? t('agentBrowser.settings.cookies.session')
      : t('agentBrowser.settings.cookies.expires', {
          date: new Date(props.cookie.expires * 1000).toLocaleString(
            locale.value,
          ),
        }),
  ]
  if (props.cookie.secure) parts.push('Secure')
  if (props.cookie.httpOnly) parts.push('HttpOnly')
  if (props.cookie.sameSite) parts.push(`SameSite=${props.cookie.sameSite}`)
  return parts.join(' · ')
})

/** A date and time for an `datetime-local` field, in the user's time zone. */
function localInput(seconds: number): string {
  const date = new Date(seconds * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

async function readValueAsync(): Promise<string | null> {
  try {
    return await invoke<string>('agent_browser_cookie_value', {
      args: ref_.value,
    })
  } catch (error) {
    toast.error(errString(error))
    return null
  }
}

async function toggleValueAsync() {
  if (value.value !== null) {
    value.value = null
    return
  }
  value.value = await readValueAsync()
}

async function startEditAsync() {
  const current = await readValueAsync()
  if (current === null) return
  draftValue.value = current
  session.value = props.cookie.expires == null
  draftExpires.value = localInput(
    props.cookie.expires ?? Math.floor(Date.now() / 1000) + 86400,
  )
  editing.value = true
}

async function saveAsync() {
  const at = session.value ? null : new Date(draftExpires.value).getTime()
  if (at !== null && Number.isNaN(at)) {
    toast.error(t('agentBrowser.settings.cookies.badExpiry'))
    return
  }
  busy.value = true
  try {
    await setCookie({
      cookie: ref_.value,
      value: draftValue.value,
      expires: at === null ? null : Math.floor(at / 1000),
    })
    editing.value = false
    value.value = null
    emit('changed')
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
  }
}

async function removeAsync() {
  busy.value = true
  try {
    await deleteCookie({ target: { cookie: ref_.value } })
    emit('changed')
  } catch (error) {
    toast.error(errString(error))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <SettingsRow
    :title="cookie.name"
    :description="description"
    data-testid="agent-browser-cookie"
    :data-cookie="`${cookie.domain}${cookie.path}:${cookie.name}`"
  >
    <UiButton
      type="button"
      size="sm"
      variant="outline"
      :disabled="busy"
      data-testid="agent-browser-cookie-show"
      @click="toggleValueAsync"
    >
      {{
        value === null
          ? t('agentBrowser.settings.cookies.show')
          : t('agentBrowser.settings.cookies.hide')
      }}
    </UiButton>
    <UiButton
      type="button"
      size="sm"
      variant="outline"
      :disabled="busy || editing"
      data-testid="agent-browser-cookie-edit"
      @click="startEditAsync"
    >
      {{ t('agentBrowser.settings.cookies.edit') }}
    </UiButton>
    <UiButton
      type="button"
      size="sm"
      variant="outline"
      :disabled="busy"
      data-testid="agent-browser-cookie-delete"
      @click="removeAsync"
    >
      {{ t('agentBrowser.settings.cookies.delete') }}
    </UiButton>
    <template #below>
      <code
        v-if="value !== null && !editing"
        class="block rounded-lg bg-muted px-3 py-2 text-xs break-all"
        data-testid="agent-browser-cookie-value"
      >
        {{ value === '' ? t('agentBrowser.settings.cookies.empty') : value }}
      </code>
      <div v-if="editing" class="flex flex-col gap-3 pt-1">
        <UiInput
          :id="`cookie-value-${cookie.domain}${cookie.path}${cookie.name}`"
          v-model="draftValue"
          :label="t('agentBrowser.settings.cookies.value')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          autocomplete="off"
          data-testid="agent-browser-cookie-value-input"
        />
        <label class="flex items-center gap-2 text-sm">
          <ShadcnCheckbox
            v-model="session"
            data-testid="agent-browser-cookie-session"
          />
          {{ t('agentBrowser.settings.cookies.sessionOnly') }}
        </label>
        <UiInput
          v-if="!session"
          :id="`cookie-expires-${cookie.domain}${cookie.path}${cookie.name}`"
          v-model="draftExpires"
          :label="t('agentBrowser.settings.cookies.expiry')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          type="datetime-local"
          data-testid="agent-browser-cookie-expires-input"
        />
        <p v-if="!session" class="text-xs text-muted-foreground">
          {{ t('agentBrowser.settings.cookies.expiryHint') }}
        </p>
        <div class="flex justify-end gap-2">
          <UiButton
            type="button"
            size="sm"
            variant="outline"
            @click="editing = false"
          >
            {{ t('agentBrowser.settings.cookies.cancel') }}
          </UiButton>
          <UiButton
            type="button"
            size="sm"
            :disabled="busy"
            data-testid="agent-browser-cookie-save"
            @click="saveAsync"
          >
            {{ t('agentBrowser.settings.cookies.save') }}
          </UiButton>
        </div>
      </div>
    </template>
  </SettingsRow>
</template>
