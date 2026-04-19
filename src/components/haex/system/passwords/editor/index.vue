<template>
  <form
    class="h-full flex flex-col overflow-hidden"
    @submit.prevent="onSave"
  >
    <div
      class="flex-none flex items-center gap-2 px-3 py-2 bg-elevated/50 backdrop-blur-md border-b border-default"
    >
      <UiButton
        :tooltip="t('back')"
        icon="i-lucide-arrow-left"
        color="neutral"
        variant="ghost"
        type="button"
        class="shrink-0"
        @click="onBack"
      />

      <div class="flex items-center gap-2 min-w-0 flex-1">
        <div
          v-if="!isCreating"
          class="shrink-0 size-8 rounded-md flex items-center justify-center bg-elevated overflow-hidden"
          :style="iconBackgroundStyle"
        >
          <UIcon
            v-if="iconDescriptor.kind === 'iconify'"
            :name="iconDescriptor.name"
            class="size-5"
            :class="form.color ? '' : 'text-primary'"
          />
          <img
            v-else-if="binaryIconSrc"
            :src="binaryIconSrc"
            :alt="form.title || 'icon'"
            class="size-6 object-contain"
          />
          <UIcon
            v-else
            name="i-lucide-key"
            class="size-5 text-muted"
          />
        </div>
        <h2 class="font-semibold truncate">
          {{
            isCreating
              ? t('titleCreate')
              : form.title || t('untitled')
          }}
        </h2>
      </div>

      <div class="flex items-center gap-1 shrink-0">
        <template v-if="isEditing">
          <UiButton
            :label="t('save')"
            icon="i-lucide-save"
            color="primary"
            type="submit"
            :loading="saving"
          />
        </template>
        <template v-else>
          <UiButton
            :tooltip="t('edit')"
            icon="i-lucide-pencil"
            color="neutral"
            variant="ghost"
            type="button"
            @click="startEdit"
          />
          <UiButton
            :tooltip="t('delete')"
            icon="i-lucide-trash-2"
            color="error"
            variant="ghost"
            type="button"
            @click="showDeleteDialog = true"
          />
        </template>
      </div>
    </div>

    <UTabs
      v-model="activeTab"
      :items="tabItems"
      class="flex-1 min-h-0 flex flex-col"
      :ui="{
        list: 'shrink-0 mx-3 my-2',
        content: 'flex-1 min-h-0 overflow-y-auto',
      }"
    >
      <!-- Details -->
      <template #details>
        <div class="p-4 space-y-4 max-w-2xl mx-auto">
          <div
            v-if="isExpired && (isEditing || form.expiresAt)"
            class="flex items-center gap-2 px-3 py-2 bg-warning/10 border border-warning/30 rounded-md text-sm"
          >
            <UIcon
              name="i-lucide-alert-triangle"
              class="size-4 text-warning shrink-0"
            />
            <span>{{ t('expired') }}</span>
          </div>

          <div v-if="isEditing || form.title">
            <UiInput
              v-model="form.title"
              v-model:errors="errors.title"
              :label="t('fields.title')"
              :placeholder="t('fields.titlePlaceholder')"
              :read-only="!isEditing"
              :required="isEditing"
              :with-copy-button="!isEditing"
            />
          </div>

          <div v-if="isEditing || form.username">
            <UiInput
              v-model="form.username"
              :label="t('fields.username')"
              leading-icon="i-lucide-user"
              :read-only="!isEditing"
              with-copy-button
            />
          </div>

          <div v-if="isEditing || form.password">
            <UiInputPassword
              v-model="form.password"
              :label="t('fields.password')"
              :read-only="!isEditing"
              with-copy-button
            />
          </div>

          <div v-if="isEditing || form.url">
            <UiInput
              v-model="form.url"
              :label="t('fields.url')"
              leading-icon="i-lucide-globe"
              type="url"
              placeholder="https://…"
              :read-only="!isEditing"
              with-copy-button
            />
          </div>

          <div v-if="isEditing || form.tagNames.length">
            <HaexSystemPasswordsEditorTagPicker
              v-if="isEditing"
              v-model="form.tagNames"
              :label="t('fields.tags')"
            />
            <div v-else>
              <p class="text-xs font-medium text-muted mb-1">
                {{ t('fields.tags') }}
              </p>
              <div class="flex flex-wrap gap-1">
                <UBadge
                  v-for="name in form.tagNames"
                  :key="name"
                  :label="name"
                  color="neutral"
                  variant="soft"
                />
              </div>
            </div>
            <p
              v-if="errors.tags.length"
              class="mt-1 text-xs text-error"
            >
              {{ errors.tags[0] }}
            </p>
          </div>

          <div v-if="isEditing || form.note">
            <UiTextarea
              v-model="form.note"
              :label="t('fields.note')"
              :rows="3"
              :read-only="!isEditing"
            />
          </div>

          <div
            v-if="isEditing || form.expiresAt || form.icon"
            class="grid grid-cols-2 gap-3"
          >
            <UiInput
              v-if="isEditing || form.expiresAt"
              v-model="form.expiresAt"
              :label="t('fields.expiresAt')"
              type="date"
              leading-icon="i-lucide-calendar"
              :read-only="!isEditing"
            />
            <UiInput
              v-if="isEditing || form.icon"
              v-model="form.icon"
              :label="t('fields.icon')"
              placeholder="i-lucide-key"
              :read-only="!isEditing"
            />
          </div>

          <div v-if="isEditing || form.color">
            <UiInput
              v-model="form.color"
              :label="t('fields.color')"
              type="color"
              :read-only="!isEditing"
            />
          </div>

          <!-- OTP -->
          <div
            v-if="isEditing || otpCode"
            class="border border-default rounded-md p-3 space-y-3"
          >
            <div class="flex items-center gap-2">
              <UIcon
                name="i-lucide-shield-check"
                class="size-4 text-primary"
              />
              <p class="text-sm font-medium">
                {{ t('fields.otp') }}
              </p>
            </div>

            <template v-if="isEditing">
              <UiInput
                v-model="form.otpSecret"
                :label="t('fields.otpSecret')"
                placeholder="JBSWY3DPEHPK3PXP"
              />
              <div class="grid grid-cols-3 gap-2">
                <UiInput
                  v-model.number="form.otpDigits"
                  :label="t('fields.otpDigits')"
                  type="number"
                  min="6"
                  max="10"
                />
                <UiInput
                  v-model.number="form.otpPeriod"
                  :label="t('fields.otpPeriod')"
                  type="number"
                  min="10"
                  max="120"
                />
                <USelect
                  v-model="form.otpAlgorithm"
                  :items="otpAlgorithms"
                  size="md"
                />
              </div>
            </template>

            <div
              v-else
              class="flex items-center gap-3 px-3 py-2 rounded-md bg-elevated/30"
            >
              <span
                class="flex-1 font-mono text-xl tracking-[0.3em] select-all"
              >{{ otpFormatted }}</span>
              <div class="relative size-8 shrink-0">
                <svg
                  viewBox="0 0 36 36"
                  class="size-8 -rotate-90"
                >
                  <circle
                    cx="18"
                    cy="18"
                    r="15.5"
                    fill="none"
                    stroke-width="2.5"
                    class="stroke-default"
                  />
                  <circle
                    cx="18"
                    cy="18"
                    r="15.5"
                    fill="none"
                    stroke-width="2.5"
                    stroke-linecap="round"
                    :stroke-dasharray="otpDashArray"
                    class="stroke-primary transition-[stroke-dasharray] duration-1000 ease-linear"
                  />
                </svg>
                <span
                  class="absolute inset-0 flex items-center justify-center text-[10px] tabular-nums"
                >{{ otpRemaining }}</span>
              </div>
              <UiButton
                :tooltip="copiedOtp ? t('copied') : t('copy')"
                :icon="copiedOtp ? 'i-lucide-check' : 'i-lucide-copy'"
                :color="copiedOtp ? 'success' : 'neutral'"
                variant="ghost"
                type="button"
                class="shrink-0"
                @click="() => copyOtp(otpCode ?? '')"
              />
            </div>
          </div>
        </div>
      </template>

      <!-- Extra -->
      <template #extra>
        <div class="p-4 space-y-3 max-w-2xl mx-auto">
          <p class="text-xs font-medium text-muted">
            {{ t('extra.description') }}
          </p>

          <div
            v-if="visibleKeyValues.length === 0"
            class="flex flex-col items-center justify-center gap-2 py-8 text-muted"
          >
            <UIcon
              name="i-lucide-list-plus"
              class="size-8 opacity-40"
            />
            <p class="text-sm">
              {{ t('extra.empty') }}
            </p>
          </div>

          <div
            v-for="(kv, index) in visibleKeyValues"
            :key="kv.id"
            class="flex items-start gap-2"
          >
            <UiInput
              v-model="kv.key"
              :placeholder="t('extra.keyPlaceholder')"
              :read-only="!isEditing"
              class="flex-1"
            />
            <UiInput
              v-model="kv.value"
              :placeholder="t('extra.valuePlaceholder')"
              :read-only="!isEditing"
              :with-copy-button="!isEditing"
              class="flex-2"
            />
            <UiButton
              v-if="isEditing"
              :tooltip="t('extra.remove')"
              icon="i-lucide-trash-2"
              color="error"
              variant="ghost"
              type="button"
              class="shrink-0 mt-0.5"
              @click="removeKeyValue(index)"
            />
          </div>

          <UiButton
            v-if="isEditing"
            :label="t('extra.add')"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            type="button"
            @click="addKeyValue"
          />
        </div>
      </template>

      <!-- History -->
      <template #history>
        <div
          class="p-6 flex flex-col items-center justify-center gap-3 text-muted h-full"
        >
          <UIcon
            name="i-lucide-history"
            class="size-12 opacity-40"
          />
          <p class="text-sm text-center">
            {{ t('history.comingSoon') }}
          </p>
        </div>
      </template>
    </UTabs>

    <HaexSystemPasswordsDialogDeleteItem
      v-model:open="showDeleteDialog"
      :item-title="form.title"
      @confirm="onDelete"
    />
  </form>
</template>

<script setup lang="ts">
import * as OTPAuth from 'otpauth'
import { useClipboard } from '@vueuse/core'
import { eq } from 'drizzle-orm'
import {
  haexPasswordsItemDetails,
  haexPasswordsItemKeyValues,
} from '~/database/schemas'
import type { InsertHaexPasswordsItemDetails } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

type EditableKeyValue = { id: string; key: string; value: string }

const { t } = useI18n()
const toast = useToast()

const passwordsStore = usePasswordsStore()
const tagsStore = usePasswordsTagsStore()
const nav = usePasswordsNavigation()
const { selectedItem, selectedItemTags, isEditing } =
  storeToRefs(passwordsStore)

const { getIconDescriptor } = useIconComponents()
const iconCacheStore = usePasswordsIconCacheStore()

const isCreating = computed(() => !selectedItem.value)
const otpAlgorithms = ['SHA1', 'SHA256', 'SHA512'] as const

const form = reactive({
  title: selectedItem.value?.title ?? '',
  username: selectedItem.value?.username ?? '',
  password: selectedItem.value?.password ?? '',
  url: selectedItem.value?.url ?? '',
  note: selectedItem.value?.note ?? '',
  icon: selectedItem.value?.icon ?? '',
  color: selectedItem.value?.color ?? '',
  expiresAt: selectedItem.value?.expiresAt?.slice(0, 10) ?? '',
  otpSecret: selectedItem.value?.otpSecret ?? '',
  otpDigits: selectedItem.value?.otpDigits ?? 6,
  otpPeriod: selectedItem.value?.otpPeriod ?? 30,
  otpAlgorithm: (selectedItem.value?.otpAlgorithm ??
    'SHA1') as (typeof otpAlgorithms)[number],
  tagNames: selectedItemTags.value.map((t) => t.name),
  keyValues: [] as EditableKeyValue[],
})

// Snapshot of the pristine form for cancel-from-edit on existing items.
const formSnapshot = JSON.parse(JSON.stringify(form)) as typeof form

const errors = reactive({
  title: [] as string[],
  tags: [] as string[],
})

const saving = ref(false)
const activeTab = ref('details')
const showDeleteDialog = ref(false)

// Register tab switches on the navigation back stack so browser-back
// walks through tabs before leaving the item view.
nav.trackHistory(activeTab)

const tabItems = computed(() => [
  { label: t('tabs.details'), value: 'details', slot: 'details' as const },
  { label: t('tabs.extra'), value: 'extra', slot: 'extra' as const },
  { label: t('tabs.history'), value: 'history', slot: 'history' as const },
])

const visibleKeyValues = computed(() =>
  isEditing.value
    ? form.keyValues
    : form.keyValues.filter((kv) => kv.key.trim() || kv.value.trim()),
)

const iconDescriptor = computed(() => getIconDescriptor(form.icon || null))

const binaryIconSrc = computed(() => {
  if (iconDescriptor.value.kind !== 'binary') return null
  const src = iconCacheStore.getIconDataUrl(iconDescriptor.value.hash)
  if (src === null) {
    iconCacheStore.requestIcon(iconDescriptor.value.hash)
    return null
  }
  return src || null
})

const iconBackgroundStyle = computed(() =>
  form.color ? { backgroundColor: form.color } : undefined,
)

const isExpired = computed(() => {
  if (!form.expiresAt) return false
  const ts = Date.parse(form.expiresAt)
  if (Number.isNaN(ts)) return false
  return ts < Date.now()
})

// OTP ticker — one shared clock.
const nowMs = ref(Date.now())
let otpTicker: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  otpTicker = setInterval(() => {
    nowMs.value = Date.now()
  }, 1000)
})
onBeforeUnmount(() => {
  if (otpTicker) clearInterval(otpTicker)
})

const totp = computed(() => {
  const secret = form.otpSecret.trim()
  if (!secret) return null
  try {
    return new OTPAuth.TOTP({
      algorithm: form.otpAlgorithm,
      digits: form.otpDigits || 6,
      period: form.otpPeriod || 30,
      secret: OTPAuth.Secret.fromBase32(secret),
    })
  } catch (error) {
    console.error('[OTP] Invalid secret', error)
    return null
  }
})

const otpCode = computed(() => {
  if (!totp.value) return null
  void nowMs.value
  return totp.value.generate()
})

const otpFormatted = computed(() => {
  if (!otpCode.value) return ''
  const mid = Math.floor(otpCode.value.length / 2)
  return `${otpCode.value.slice(0, mid)} ${otpCode.value.slice(mid)}`
})

const otpRemaining = computed(() => {
  const secs = Math.floor(nowMs.value / 1000)
  return (form.otpPeriod || 30) - (secs % (form.otpPeriod || 30))
})

const OTP_CIRCUMFERENCE = 2 * Math.PI * 15.5
const otpDashArray = computed(() => {
  const progress = otpRemaining.value / (form.otpPeriod || 30)
  return `${progress * OTP_CIRCUMFERENCE} ${OTP_CIRCUMFERENCE}`
})

const { copy: copyToClipboard, copied: copiedOtp } = useClipboard({
  copiedDuring: 1500,
})
const copyOtp = (value: string) => copyToClipboard(value)

const loadKeyValuesAsync = async () => {
  if (!selectedItem.value?.id) return
  const db = requireDb()
  const rows = await db
    .select()
    .from(haexPasswordsItemKeyValues)
    .where(eq(haexPasswordsItemKeyValues.itemId, selectedItem.value.id))
  form.keyValues = rows.map((row) => ({
    id: row.id,
    key: row.key ?? '',
    value: row.value ?? '',
  }))
  formSnapshot.keyValues = JSON.parse(JSON.stringify(form.keyValues))
}

onMounted(async () => {
  try {
    await tagsStore.loadTagsAsync()
  } catch (error) {
    console.error('[Editor] Failed to load tags:', error)
  }
  try {
    await loadKeyValuesAsync()
  } catch (error) {
    console.error('[Editor] Failed to load key-values:', error)
  }
})

const startEdit = () => {
  nav.startEdit()
}

const addKeyValue = () => {
  form.keyValues.push({ id: crypto.randomUUID(), key: '', value: '' })
}

const removeKeyValue = (index: number) => {
  form.keyValues.splice(index, 1)
}

const revertForm = () => {
  Object.assign(form, JSON.parse(JSON.stringify(formSnapshot)))
  errors.title = []
  errors.tags = []
}

const onBack = () => {
  // Existing-item edit → revert unsaved changes; create-cancel is a hard
  // drop to list, handled by the popped navigation state.
  if (isEditing.value && !isCreating.value) {
    revertForm()
  }
  nav.goBack()
}

const onDelete = async () => {
  if (!selectedItem.value) return
  const id = selectedItem.value.id
  try {
    await passwordsStore.deleteItemAsync(id)
    showDeleteDialog.value = false
    passwordsStore.backToList()
    toast.add({ title: t('toast.deleted'), color: 'success' })
  } catch (error) {
    console.error('[Editor] Delete failed:', error)
    toast.add({
      title: t('toast.deleteError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const onSave = async () => {
  errors.title = []
  errors.tags = []

  if (!form.title.trim()) {
    errors.title = [t('validation.titleRequired')]
    activeTab.value = 'details'
    return
  }

  saving.value = true
  try {
    const db = requireDb()
    const itemId = selectedItem.value?.id ?? crypto.randomUUID()
    const now = new Date().toISOString()

    const payload: InsertHaexPasswordsItemDetails = {
      id: itemId,
      title: form.title.trim(),
      username: form.username.trim() || null,
      password: form.password || null,
      url: form.url.trim() || null,
      note: form.note || null,
      icon: form.icon.trim() || null,
      color: form.color || null,
      expiresAt: form.expiresAt || null,
      otpSecret: form.otpSecret.trim() || null,
      otpDigits: form.otpDigits || 6,
      otpPeriod: form.otpPeriod || 30,
      otpAlgorithm: form.otpAlgorithm,
      updatedAt: now,
    }

    if (isCreating.value) {
      await db
        .insert(haexPasswordsItemDetails)
        .values({ ...payload, createdAt: now })
    } else {
      await db
        .update(haexPasswordsItemDetails)
        .set(payload)
        .where(eq(haexPasswordsItemDetails.id, itemId))
    }

    const resolvedTags = await tagsStore.resolveTagNamesAsync(form.tagNames)
    await tagsStore.setItemTagsAsync(
      itemId,
      resolvedTags.map((tag) => tag.id),
    )

    // Key-values: delete the full set, then re-insert non-empty rows.
    await db
      .delete(haexPasswordsItemKeyValues)
      .where(eq(haexPasswordsItemKeyValues.itemId, itemId))
    for (const kv of form.keyValues) {
      if (!kv.key.trim()) continue
      await db.insert(haexPasswordsItemKeyValues).values({
        id: kv.id,
        itemId,
        key: kv.key.trim(),
        value: kv.value,
        updatedAt: now,
      })
    }

    await passwordsStore.loadItemsAsync()
    passwordsStore.openItem(itemId)

    // Refresh the snapshot to the newly saved state.
    Object.assign(formSnapshot, JSON.parse(JSON.stringify(form)))

    toast.add({
      title: isCreating.value ? t('toast.created') : t('toast.updated'),
      color: 'success',
    })
  } catch (error) {
    console.error('[Editor] Save failed:', error)
    toast.add({
      title: t('toast.saveError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  } finally {
    saving.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  titleCreate: Neuer Eintrag
  untitled: (ohne Titel)
  back: Zurück
  edit: Bearbeiten
  delete: Löschen
  save: Speichern
  copy: Kopieren
  copied: Kopiert
  expired: Dieser Eintrag ist abgelaufen.
  tabs:
    details: Details
    extra: Extra
    history: Verlauf
  fields:
    title: Titel
    titlePlaceholder: z.B. GitHub
    tags: Tags
    username: Nutzername
    password: Passwort
    url: URL
    note: Notiz
    expiresAt: Ablaufdatum
    icon: Icon
    color: Farbe
    otp: Einmalcode (TOTP)
    otpSecret: Base32 Secret
    otpDigits: Stellen
    otpPeriod: Periode (s)
  extra:
    description: Eigene Felder (z.B. Recovery-Code, PIN, Sicherheitsfragen).
    empty: Noch keine eigenen Felder.
    add: Feld hinzufügen
    remove: Feld entfernen
    keyPlaceholder: Schlüssel
    valuePlaceholder: Wert
  history:
    comingSoon: Verlauf-Ansicht kommt mit den Snapshots in Etappe 3.
  validation:
    titleRequired: Titel ist Pflicht
  toast:
    created: Eintrag erstellt
    updated: Eintrag aktualisiert
    deleted: Eintrag gelöscht
    saveError: Speichern fehlgeschlagen
    deleteError: Löschen fehlgeschlagen
en:
  titleCreate: New entry
  untitled: (untitled)
  back: Back
  edit: Edit
  delete: Delete
  save: Save
  copy: Copy
  copied: Copied
  expired: This entry has expired.
  tabs:
    details: Details
    extra: Extra
    history: History
  fields:
    title: Title
    titlePlaceholder: e.g. GitHub
    tags: Tags
    username: Nutzername
    password: Password
    url: URL
    note: Note
    expiresAt: Expires at
    icon: Icon
    color: Color
    otp: One-time code (TOTP)
    otpSecret: Base32 secret
    otpDigits: Digits
    otpPeriod: Period (s)
  extra:
    description: Custom fields (e.g. recovery code, PIN, security questions).
    empty: No custom fields yet.
    add: Add field
    remove: Remove field
    keyPlaceholder: Key
    valuePlaceholder: Value
  history:
    comingSoon: History view ships with snapshots in stage 3.
  validation:
    titleRequired: Title is required
  toast:
    created: Entry created
    updated: Entry updated
    deleted: Entry deleted
    saveError: Saving failed
    deleteError: Deletion failed
</i18n>
