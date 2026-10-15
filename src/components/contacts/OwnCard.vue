<script setup lang="ts">
/**
 * The vault's own Visitenkarte (spec 051 FR-009, US4-1): QR code and link with the name and the
 * values the person chose, and when it was made. A main device makes it; a linked device shows
 * the one a main device made. What the card carries is chosen in Einstellungen › Kontakte.
 */
import { toast } from 'vue-sonner'
import type { ContactsOwnCard } from '@bindings/ContactsOwnCard'

const { t, d } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const { openApp } = useWmTab()
const { ownCardAsync } = useContactCards()

const card = ref<ContactsOwnCard | null>(null)
const error = ref<string | null>(null)

async function loadAsync() {
  try {
    card.value = await ownCardAsync()
    error.value = null
  } catch (cause) {
    error.value = errString(cause)
  }
}
onMounted(loadAsync)
// A main device renews the card when devices or relays change; another device may do it too.
onVaultTablesChanged(['haex_contacts_own_card', 'preferences'], loadAsync)

const qrSource = computed(() =>
  card.value?.qrSvg
    ? `data:image/svg+xml;utf8,${encodeURIComponent(card.value.qrSvg)}`
    : '',
)

async function copyAsync() {
  if (!card.value?.text) return
  try {
    await navigator.clipboard.writeText(card.value.text)
    toast.success(t('contacts.card.copied'))
  } catch {
    toast.error(t('contacts.copyFailed'))
  }
}
</script>

<template>
  <div class="flex min-h-0 flex-col gap-4 p-3" data-testid="contacts-own-card">
    <div class="flex items-center gap-1">
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('contacts.back')"
        :tooltip="t('contacts.back')"
        data-testid="contacts-back"
        @click="router.back()"
      >
        <Icon name="lucide:arrow-left" class="size-4" />
      </UiButton>
      <h1 class="min-w-0 flex-1 truncate px-1 text-2xl font-semibold">
        {{ t('contacts.card.own') }}
      </h1>
    </div>

    <p
      v-if="error"
      class="rounded-lg border border-destructive/40 p-3 text-sm text-destructive"
      role="alert"
    >
      {{ error }}
    </p>

    <template v-else-if="card">
      <p
        v-if="!card.text"
        class="p-4 text-muted-foreground"
        data-testid="contacts-own-card-none"
      >
        {{ t('contacts.card.noneYet') }}
      </p>
      <template v-else>
        <div class="flex flex-col items-center gap-3">
          <img
            :src="qrSource"
            :alt="t('contacts.card.qr')"
            class="size-64 rounded-lg bg-background p-2"
            data-testid="contacts-own-card-qr"
          />
          <p class="text-sm text-muted-foreground">
            {{
              t('contacts.card.issuedAt', {
                at: card.issuedAt
                  ? d(new Date(card.issuedAt), {
                      dateStyle: 'medium',
                      timeStyle: 'short',
                    })
                  : '',
              })
            }}
          </p>
          <div class="flex gap-2">
            <UiButton data-testid="contacts-own-card-copy" @click="copyAsync">
              <Icon name="lucide:copy" class="size-4" />
              {{ t('contacts.card.copy') }}
            </UiButton>
          </div>
          <code
            class="max-w-full break-all rounded-lg bg-muted p-2 text-xs"
            data-testid="contacts-own-card-text"
            >{{ card.text }}</code
          >
        </div>
        <p
          v-if="!card.canSign"
          class="text-sm text-muted-foreground"
          data-testid="contacts-own-card-linked"
        >
          {{ t('contacts.card.linkedDevice') }}
        </p>
        <p
          v-if="card.omitted.length"
          class="text-sm text-muted-foreground"
          data-testid="contacts-own-card-omitted"
        >
          {{ t('contacts.card.omitted', { values: card.omitted.join(', ') }) }}
        </p>
      </template>
      <p v-if="!card.ownContact" class="text-sm text-muted-foreground">
        {{ t('contacts.card.noOwnContact') }}
      </p>
      <div>
        <UiButton
          variant="outline"
          data-testid="contacts-own-card-settings"
          @click="openApp('system.settings', '/contacts')"
        >
          <Icon name="lucide:settings-2" class="size-4" />
          {{ t('contacts.card.choose') }}
        </UiButton>
      </div>
    </template>
  </div>
</template>
