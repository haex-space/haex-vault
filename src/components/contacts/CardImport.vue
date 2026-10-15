<script setup lang="ts">
/**
 * Reading a Visitenkarte in (spec 051 FR-011, US4-2/3): paste its link, or scan its QR code on a
 * phone. holzi checks the card first and shows a preview; nothing is stored until the person
 * chooses "Neuer Kontakt" or a contact to add it to. A vault whose card a contact already has goes
 * only to that contact (FR-013).
 */
import { toast } from 'vue-sonner'
import type { ContactsCardInto } from '@bindings/ContactsCardInto'
import type { ContactsCardPreview } from '@bindings/ContactsCardPreview'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useContactsStore()
const { isAndroid } = useDeviceCapabilities()
const { kindName, labelName } = useContactLabels()
const { previewAsync, acceptAsync, blockAsync } = useContactCards()

const text = ref('')
const preview = ref<ContactsCardPreview | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)
const into = ref('')

async function checkAsync() {
  error.value = null
  preview.value = null
  if (!text.value.trim()) return
  busy.value = true
  try {
    preview.value = await previewAsync(text.value)
  } catch (cause) {
    error.value = errString(cause)
  } finally {
    busy.value = false
  }
}

async function scanAsync() {
  try {
    const { scan, Format } = await import('@tauri-apps/plugin-barcode-scanner')
    const scanned = await scan({ formats: [Format.QRCode] })
    text.value = scanned.content
    await checkAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function acceptAsync_(target: ContactsCardInto) {
  busy.value = true
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // choice after the preview calls the command.
    const contact = await acceptAsync(text.value, target)
    await store.quietReloadAsync()
    toast.success(t('contacts.card.accepted', { name: contact.name }))
    await router.replace(`/entry/${contact.id}`)
  } catch (cause) {
    error.value = errString(cause)
  } finally {
    busy.value = false
  }
}

async function blockAsync_() {
  if (!preview.value) return
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // button of the preview calls the command.
    await blockAsync(preview.value.vaultPubkey)
    await checkAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

const others = computed(() => [
  { value: '', label: t('contacts.card.addTo') },
  ...store.headers.map((header) => ({
    value: header.id,
    label: header.name || t('contacts.noName'),
  })),
])
</script>

<template>
  <div
    class="flex min-h-0 flex-col gap-4 p-3"
    data-testid="contacts-card-import"
  >
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
        {{ t('contacts.card.import') }}
      </h1>
    </div>

    <form class="flex flex-col gap-2" @submit.prevent="checkAsync">
      <UiTextarea
        id="contacts-card-text"
        v-model="text"
        :label="t('contacts.card.paste')"
        label-bg="var(--background)"
        rows="3"
        data-testid="contacts-card-text"
      />
      <div class="flex flex-wrap gap-2">
        <UiButton
          type="submit"
          :loading="busy && !preview"
          data-testid="contacts-card-check"
        >
          {{ t('contacts.card.check') }}
        </UiButton>
        <UiButton
          v-if="isAndroid"
          type="button"
          variant="outline"
          data-testid="contacts-card-scan"
          @click="scanAsync"
        >
          <Icon name="lucide:scan-qr-code" class="size-4" />
          {{ t('contacts.card.scan') }}
        </UiButton>
      </div>
    </form>

    <p
      v-if="error"
      class="rounded-lg border border-destructive/40 p-3 text-sm text-destructive"
      role="alert"
      data-testid="contacts-card-error"
    >
      {{ error }}
    </p>

    <template v-if="preview">
      <SettingsGroup
        :label="t('contacts.card.preview')"
        data-testid="contacts-card-preview"
      >
        <SettingsRow
          :title="preview.name || t('contacts.noName')"
          :description="preview.npub"
          data-testid="contacts-card-preview-name"
        />
        <SettingsRow
          :title="
            t(
              'contacts.card.devices',
              { count: preview.devices },
              preview.devices,
            )
          "
          :description="preview.nostrRelays.join(', ')"
        />
        <SettingsRow
          v-for="(value, index) in preview.values"
          :key="index"
          :title="value.value"
          :description="`${kindName(value.kind)} · ${labelName(value.label)}`"
        />
      </SettingsGroup>
      <p
        v-if="preview.blocked"
        class="text-sm text-muted-foreground"
        data-testid="contacts-card-blocked"
      >
        {{ t('contacts.card.blockedNote') }}
      </p>
      <div class="flex flex-wrap items-center gap-2">
        <UiButton
          v-if="preview.existingContact"
          :loading="busy"
          data-testid="contacts-card-update"
          @click="
            acceptAsync_({
              kind: 'contact',
              contactId: preview.existingContact.id,
            })
          "
        >
          {{
            t('contacts.card.update', { name: preview.existingContact.name })
          }}
        </UiButton>
        <template v-else>
          <UiButton
            :loading="busy"
            data-testid="contacts-card-new"
            @click="acceptAsync_({ kind: 'new' })"
          >
            {{ t('contacts.card.asNew') }}
          </UiButton>
          <SettingsSelect
            v-if="others.length > 1"
            id="contacts-card-into"
            v-model="into"
            :options="others"
            data-testid="contacts-card-into"
          />
          <UiButton
            v-if="into"
            variant="outline"
            :loading="busy"
            data-testid="contacts-card-add"
            @click="acceptAsync_({ kind: 'contact', contactId: into })"
          >
            {{ t('contacts.card.add') }}
          </UiButton>
        </template>
        <UiButton
          v-if="!preview.blocked"
          variant="ghost"
          data-testid="contacts-card-block"
          @click="blockAsync_"
        >
          {{ t('contacts.card.block') }}
        </UiButton>
      </div>
    </template>
  </div>
</template>
