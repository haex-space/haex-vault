<template>
  <form
    class="h-full flex flex-col overflow-hidden"
    @submit.prevent="onSubmit"
  >
    <!-- Sticky header with Save / Cancel -->
    <div
      class="sticky top-0 z-10 flex items-center gap-2 px-3 py-2 bg-elevated/50 backdrop-blur-md border-b border-default"
    >
      <UiButton
        :tooltip="t('cancel')"
        icon="i-lucide-arrow-left"
        color="neutral"
        variant="ghost"
        type="button"
        class="shrink-0"
        @click="onCancel"
      />
      <h2 class="font-semibold flex-1 truncate">
        {{ isCreating ? t('titleCreate') : t('titleEdit') }}
      </h2>
      <UiButton
        :label="t('save')"
        icon="i-lucide-save"
        color="primary"
        size="sm"
        type="submit"
        :loading="saving"
      />
    </div>

    <div class="flex-1 overflow-y-auto p-4 space-y-4">
      <UiInput
        v-model="form.title"
        v-model:errors="errors.title"
        :label="t('fields.title')"
        :placeholder="t('fields.titlePlaceholder')"
        required
      />

      <div>
        <p class="text-xs font-medium text-muted mb-1">
          {{ t('fields.tags') }} <span class="text-error">*</span>
        </p>
        <HaexSystemPasswordsEditorTagPicker v-model="form.tagNames" />
        <p
          v-if="errors.tags.length"
          class="mt-1 text-xs text-error"
        >
          {{ errors.tags[0] }}
        </p>
      </div>

      <UiInput
        v-model="form.username"
        :label="t('fields.username')"
        leading-icon="i-lucide-user"
      />

      <UiInputPassword
        v-model="form.password"
        :label="t('fields.password')"
      />

      <UiInput
        v-model="form.url"
        :label="t('fields.url')"
        leading-icon="i-lucide-globe"
        type="url"
        placeholder="https://…"
      />

      <UiTextarea
        v-model="form.note"
        :label="t('fields.note')"
        :rows="3"
      />

      <div class="grid grid-cols-2 gap-3">
        <UiInput
          v-model="form.expiresAt"
          :label="t('fields.expiresAt')"
          type="date"
          leading-icon="i-lucide-calendar"
        />
        <UiInput
          v-model="form.icon"
          :label="t('fields.icon')"
          placeholder="i-lucide-key"
        />
      </div>

      <UiInput
        v-model="form.color"
        :label="t('fields.color')"
        type="color"
      />

      <!-- OTP -->
      <div class="border border-default rounded-md p-3 space-y-3">
        <div class="flex items-center gap-2">
          <UIcon
            name="i-lucide-shield-check"
            class="size-4 text-primary"
          />
          <p class="text-sm font-medium">
            {{ t('fields.otp') }}
          </p>
        </div>
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
      </div>
    </div>
  </form>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexPasswordsItemDetails } from '~/database/schemas'
import type { InsertHaexPasswordsItemDetails } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

const { t } = useI18n()
const toast = useToast()

const passwordsStore = usePasswordsStore()
const tagsStore = usePasswordsTagsStore()
const { selectedItem, selectedItemTags } = storeToRefs(passwordsStore)

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
})

const errors = reactive({
  title: [] as string[],
  tags: [] as string[],
})

const saving = ref(false)

onMounted(async () => {
  try {
    await tagsStore.loadTagsAsync()
  } catch (error) {
    console.error('[Editor] Failed to load tags:', error)
  }
})

const onCancel = () => {
  if (isCreating.value) {
    passwordsStore.backToList()
  } else {
    // back to the item's detail view
    passwordsStore.openItem(selectedItem.value!.id)
  }
}

const onSubmit = async () => {
  errors.title = []
  errors.tags = []

  if (!form.title.trim()) {
    errors.title = [t('validation.titleRequired')]
    return
  }
  if (form.tagNames.length === 0) {
    errors.tags = [t('validation.tagRequired')]
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

    await passwordsStore.loadItemsAsync()
    passwordsStore.openItem(itemId)

    toast.add({
      title: isCreating.value ? t('toast.created') : t('toast.updated'),
      color: 'success',
    })
  } catch (error) {
    console.error('[Editor] Save failed:', error)
    toast.add({
      title: t('toast.saveError'),
      description:
        error instanceof Error ? error.message : String(error),
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
  titleEdit: Eintrag bearbeiten
  cancel: Abbrechen
  save: Speichern
  fields:
    title: Titel
    titlePlaceholder: z.B. GitHub
    tags: Tags
    username: Benutzername
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
  validation:
    titleRequired: Titel ist Pflicht
    tagRequired: Mindestens ein Tag ist Pflicht
  toast:
    created: Eintrag erstellt
    updated: Eintrag aktualisiert
    saveError: Speichern fehlgeschlagen
en:
  titleCreate: New entry
  titleEdit: Edit entry
  cancel: Cancel
  save: Save
  fields:
    title: Title
    titlePlaceholder: e.g. GitHub
    tags: Tags
    username: Username
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
  validation:
    titleRequired: Title is required
    tagRequired: At least one tag is required
  toast:
    created: Entry created
    updated: Entry updated
    saveError: Saving failed
</i18n>
