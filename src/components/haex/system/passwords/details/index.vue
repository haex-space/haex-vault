<template>
  <div
    v-if="item"
    class="h-full overflow-y-auto"
  >
    <!-- Back / title bar -->
    <div
      class="sticky top-0 z-10 flex items-center gap-2 px-3 py-2 bg-elevated/50 backdrop-blur-md border-b border-default"
    >
      <UiButton
        :tooltip="t('back')"
        icon="i-lucide-arrow-left"
        color="neutral"
        variant="ghost"
        class="shrink-0"
        @click="back"
      />
      <div class="flex items-center gap-2 min-w-0 flex-1">
        <div
          class="shrink-0 size-8 rounded-md flex items-center justify-center bg-elevated overflow-hidden"
          :style="item.color ? { backgroundColor: item.color } : undefined"
        >
          <UIcon
            v-if="iconDescriptor.kind === 'iconify'"
            :name="iconDescriptor.name"
            class="size-5"
            :class="item.color ? '' : 'text-primary'"
          />
          <img
            v-else-if="binaryIconSrc"
            :src="binaryIconSrc"
            :alt="item.title ?? 'icon'"
            class="size-6 object-contain"
          />
          <UIcon
            v-else
            name="i-lucide-key"
            class="size-5 text-muted"
          />
        </div>
        <h2 class="font-semibold truncate">
          {{ item.title || t('untitled') }}
        </h2>
      </div>
      <div class="flex items-center gap-1 shrink-0">
        <UiButton
          :tooltip="t('edit')"
          icon="i-lucide-pencil"
          color="neutral"
          variant="ghost"
          size="sm"
          @click="onEdit"
        />
        <UiButton
          :tooltip="t('delete')"
          icon="i-lucide-trash-2"
          color="error"
          variant="ghost"
          size="sm"
          @click="showDeleteDialog = true"
        />
      </div>
    </div>

    <HaexSystemPasswordsDialogDeleteItem
      v-model:open="showDeleteDialog"
      :item-title="item.title ?? ''"
      @confirm="onDelete"
    />

    <div class="p-4 space-y-4">
      <!-- Tags -->
      <div
        v-if="tags.length"
        class="flex flex-wrap gap-1"
      >
        <UBadge
          v-for="tag in tags"
          :key="tag.id"
          :label="tag.name"
          color="neutral"
          variant="soft"
          size="sm"
        />
      </div>

      <!-- Expiry warning -->
      <div
        v-if="isExpired"
        class="flex items-center gap-2 px-3 py-2 bg-warning/10 border border-warning/30 rounded-md text-sm"
      >
        <UIcon
          name="i-lucide-alert-triangle"
          class="size-4 text-warning shrink-0"
        />
        <span>{{ t('expired') }}</span>
      </div>

      <!-- Username -->
      <div v-if="item.username">
        <p class="text-xs font-medium text-muted mb-1">
          {{ t('username') }}
        </p>
        <UiInput
          :model-value="item.username"
          read-only
          leading-icon="i-lucide-user"
          with-copy-button
        />
      </div>

      <!-- Password -->
      <div v-if="item.password">
        <p class="text-xs font-medium text-muted mb-1">
          {{ t('password') }}
        </p>
        <UiInputPassword
          :model-value="item.password"
          read-only
          with-copy-button
        />
      </div>

      <!-- URL -->
      <div v-if="item.url">
        <p class="text-xs font-medium text-muted mb-1">
          {{ t('url') }}
        </p>
        <div class="flex items-center gap-2 text-sm">
          <UIcon
            name="i-lucide-globe"
            class="size-4 shrink-0 text-muted"
          />
          <a
            :href="item.url"
            target="_blank"
            rel="noopener noreferrer"
            class="text-primary hover:underline truncate"
          >
            {{ item.url }}
          </a>
        </div>
      </div>

      <!-- OTP -->
      <div v-if="otpCode">
        <p class="text-xs font-medium text-muted mb-1">
          {{ t('otp') }}
        </p>
        <div
          class="flex items-center gap-3 px-3 py-2 rounded-md border border-default bg-elevated/30"
        >
          <UIcon
            name="i-lucide-shield-check"
            class="size-5 text-primary shrink-0"
          />
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
            size="sm"
            class="shrink-0"
            @click="() => copyOtp(otpCode ?? '')"
          />
        </div>
      </div>

      <!-- Note -->
      <div v-if="item.note">
        <p class="text-xs font-medium text-muted mb-1">
          {{ t('note') }}
        </p>
        <div
          class="p-3 rounded-md border border-default bg-elevated/30 text-sm whitespace-pre-wrap"
        >
          {{ item.note }}
        </div>
      </div>
    </div>
  </div>
  <div
    v-else
    class="h-full flex flex-col items-center justify-center gap-3 text-muted"
  >
    <UIcon
      name="i-lucide-inbox"
      class="size-12 opacity-40"
    />
    <p class="text-sm">
      {{ t('noSelection') }}
    </p>
    <UiButton
      variant="outline"
      color="neutral"
      :label="t('back')"
      icon="i-lucide-arrow-left"
      @click="back"
    />
  </div>
</template>

<script setup lang="ts">
import * as OTPAuth from 'otpauth'
import { useClipboard } from '@vueuse/core'

const { t } = useI18n()
const toast = useToast()
const passwordsStore = usePasswordsStore()
const { selectedItem: item, selectedItemTags: tags } = storeToRefs(passwordsStore)
const { backToList: back, startEdit, deleteItemAsync } = passwordsStore

const showDeleteDialog = ref(false)

const onEdit = () => {
  if (item.value) startEdit(item.value.id)
}

const onDelete = async () => {
  if (!item.value) return
  const id = item.value.id
  try {
    await deleteItemAsync(id)
    showDeleteDialog.value = false
    back()
    toast.add({
      title: t('deleted'),
      color: 'success',
    })
  } catch (error) {
    console.error('[Details] Delete failed:', error)
    toast.add({
      title: t('deleteError'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const { getIconDescriptor } = useIconComponents()
const iconCacheStore = usePasswordsIconCacheStore()

const iconDescriptor = computed(() =>
  getIconDescriptor(item.value?.icon ?? null),
)

const binaryIconSrc = computed(() => {
  if (iconDescriptor.value.kind !== 'binary') return null
  const src = iconCacheStore.getIconDataUrl(iconDescriptor.value.hash)
  if (src === null) {
    iconCacheStore.requestIcon(iconDescriptor.value.hash)
    return null
  }
  return src || null
})

const isExpired = computed(() => {
  if (!item.value?.expiresAt) return false
  const ts = Date.parse(item.value.expiresAt)
  if (Number.isNaN(ts)) return false
  return ts < Date.now()
})

// OTP logic — recompute every second from a shared clock ref.
const now = ref(Date.now())
let otpTicker: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  otpTicker = setInterval(() => {
    now.value = Date.now()
  }, 1000)
})
onBeforeUnmount(() => {
  if (otpTicker) clearInterval(otpTicker)
})

const totp = computed(() => {
  const secret = item.value?.otpSecret
  if (!secret) return null
  try {
    return new OTPAuth.TOTP({
      algorithm: item.value?.otpAlgorithm ?? 'SHA1',
      digits: item.value?.otpDigits ?? 6,
      period: item.value?.otpPeriod ?? 30,
      secret: OTPAuth.Secret.fromBase32(secret),
    })
  } catch (error) {
    console.error('[OTP] Invalid secret for item', item.value?.id, error)
    return null
  }
})

const otpPeriod = computed(() => item.value?.otpPeriod ?? 30)

const otpCode = computed(() => {
  if (!totp.value) return null
  void now.value // depend on ticker
  return totp.value.generate()
})

// Split a 6-digit code into two groups of three (123 456) — classic TOTP UX.
const otpFormatted = computed(() => {
  if (!otpCode.value) return ''
  const code = otpCode.value
  const mid = Math.floor(code.length / 2)
  return `${code.slice(0, mid)} ${code.slice(mid)}`
})

const otpRemaining = computed(() => {
  const secs = Math.floor(now.value / 1000)
  return otpPeriod.value - (secs % otpPeriod.value)
})

// Circumference of r=15.5 ≈ 2π·15.5 ≈ 97.389.
const OTP_CIRCUMFERENCE = 2 * Math.PI * 15.5
const otpDashArray = computed(() => {
  const progress = otpRemaining.value / otpPeriod.value
  return `${progress * OTP_CIRCUMFERENCE} ${OTP_CIRCUMFERENCE}`
})

const { copy: copyToClipboard, copied: copiedOtp } = useClipboard({
  copiedDuring: 1500,
})
const copyOtp = (value: string) => copyToClipboard(value)
</script>

<i18n lang="yaml">
de:
  back: Zurück
  edit: Bearbeiten
  delete: Löschen
  deleted: Eintrag gelöscht
  deleteError: Löschen fehlgeschlagen
  untitled: (ohne Titel)
  username: Benutzername
  password: Passwort
  url: URL
  otp: Einmalcode (TOTP)
  note: Notiz
  expired: Dieser Eintrag ist abgelaufen.
  noSelection: Kein Eintrag ausgewählt.
  copy: Kopieren
  copied: Kopiert
en:
  back: Back
  edit: Edit
  delete: Delete
  deleted: Entry deleted
  deleteError: Deletion failed
  untitled: (untitled)
  username: Username
  password: Password
  url: URL
  otp: One-time code (TOTP)
  note: Note
  expired: This entry has expired.
  noSelection: No entry selected.
  copy: Copy
  copied: Copied
</i18n>
