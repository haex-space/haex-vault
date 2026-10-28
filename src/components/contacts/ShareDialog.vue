<script setup lang="ts">
/**
 * The share dialog of a contact (spec 060 FR-008 to FR-019), the same for the own contact and for
 * every other: every row chosen when it opens and nothing remembered (FR-010), the text of the
 * share rebuilt with every change of the choice. The text is link, clipboard text and QR code at
 * once; the QR code only shows for a text that fits one.
 *
 * In `confirm` mode (spec 060 FR-029 to FR-032) an agent wants to share the contact: the dialog opens
 * with the agent's proposal chosen and the options it was given, shows no text, and answers with the
 * choice the person left standing ("Freigeben") or none ("Ablehnen"). The owner of the request
 * closes the dialog by dropping the request; closing it any other way is its to treat as a "no".
 */
import { toast } from 'vue-sonner'
import type { ContactShareOptions } from '@bindings/ContactShareOptions'
import type { ContactShareSelection } from '@bindings/ContactShareSelection'
import type { ContactShareOutput } from '@bindings/ContactShareOutput'
import type { VaultDevice } from '@bindings/VaultDevice'
import {
  canBuild,
  selectAll,
  selectNone,
  toArgs,
  type ShareSelection,
} from '~/lib/contacts/share'

const props = withDefaults(
  defineProps<{
    contactId: string
    mode?: 'share' | 'confirm'
    /** `confirm`: what the agent was offered, and what it proposes inside it. */
    offered?: ContactShareOptions
    proposed?: ContactShareSelection
    /** `confirm`: the name of the contact in the intro. */
    contactName?: string
  }>(),
  { mode: 'share', offered: undefined, proposed: undefined, contactName: '' },
)

const emit = defineEmits<{
  confirm: [selection: ShareSelection]
  decline: []
}>()

const open = defineModel<boolean>('open', { required: true })
const confirming = computed(() => props.mode === 'confirm')

const { t } = useI18n()
const { errString } = useErrorString()
const { shareOptionsAsync, shareBuildAsync } = useContactShare()
const { listVaultDevicesAsync } = useDevice()

const options = ref<ContactShareOptions | null>(null)
const selection = ref<ShareSelection | null>(null)
const output = ref<ContactShareOutput | null>(null)
const error = ref<string | null>(null)
const devices = ref<VaultDevice[]>([])
let generation = 0

/** The name of a device of this vault: this one, its alias, or "another device". */
function deviceName(uuid: string): string {
  const device = devices.value.find(
    (candidate) => candidate.vaultDeviceUuid === uuid,
  )
  if (device?.isCurrent) return t('contacts.origin.thisDevice')
  return device?.alias ?? t('contacts.origin.otherDevice')
}

watch(
  open,
  async (isOpen) => {
    generation += 1
    options.value = null
    selection.value = null
    output.value = null
    error.value = null
    if (!isOpen) return
    const mine = generation
    try {
      const [loaded, known] = await Promise.all([
        confirming.value && props.offered
          ? Promise.resolve(props.offered)
          : shareOptionsAsync(props.contactId),
        listVaultDevicesAsync().catch(() => [] as VaultDevice[]),
      ])
      if (mine !== generation) return
      devices.value = known
      options.value = loaded
      selection.value =
        confirming.value && props.proposed
          ? toArgs(props.proposed)
          : selectAll(loaded)
    } catch (cause) {
      if (mine === generation) error.value = errString(cause)
    }
  },
  { immediate: true },
)

function allow() {
  if (selection.value) emit('confirm', toArgs(selection.value))
}

const buildable = computed(
  () =>
    options.value !== null &&
    selection.value !== null &&
    canBuild(options.value, selection.value),
)

// The text follows every change of the choice (the backend is local and quick).
watch(
  [selection, buildable],
  async () => {
    if (confirming.value || !selection.value || !options.value) return
    generation += 1
    const mine = generation
    output.value = null
    error.value = null
    if (!buildable.value) return
    try {
      const built = await shareBuildAsync(
        props.contactId,
        toArgs(selection.value),
      )
      if (mine === generation) output.value = built
    } catch (cause) {
      if (mine === generation) error.value = errString(cause)
    }
  },
  { deep: true },
)

const qrSource = computed(() =>
  output.value?.qrSvg
    ? `data:image/svg+xml;utf8,${encodeURIComponent(output.value.qrSvg)}`
    : '',
)

async function copyAsync() {
  if (!output.value) return
  try {
    await navigator.clipboard.writeText(output.value.link)
    toast.success(t('contacts.share.copied'))
  } catch {
    toast.error(t('contacts.copyFailed'))
  }
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="
      confirming ? t('contacts.share.confirm.title') : t('contacts.share.title')
    "
  >
    <template #content>
      <div class="flex flex-col gap-4" data-testid="contacts-share">
        <template v-if="options && selection">
          <p
            v-if="confirming"
            class="text-sm"
            data-testid="contacts-share-confirm-intro"
          >
            {{ t('contacts.share.confirm.intro', { name: contactName }) }}
          </p>
          <p v-else class="text-sm text-muted-foreground">
            {{ t('contacts.share.intro') }}
          </p>
          <p class="text-sm" data-testid="contacts-share-name">
            {{
              options.name
                ? t('contacts.share.name', { name: options.name })
                : t('contacts.share.noName')
            }}
          </p>
          <div class="flex gap-2">
            <UiButton
              type="button"
              variant="outline"
              size="sm"
              data-testid="contacts-share-all"
              @click="selection = selectAll(options)"
            >
              {{ t('contacts.share.all') }}
            </UiButton>
            <UiButton
              type="button"
              variant="outline"
              size="sm"
              data-testid="contacts-share-none"
              @click="selection = selectNone(options)"
            >
              {{ t('contacts.share.none') }}
            </UiButton>
          </div>

          <ContactsShareChoice
            v-model:selection="selection"
            :options="options"
            :device-name="deviceName"
          />

          <p class="text-sm text-muted-foreground">
            {{ t('contacts.share.photoHint') }}
          </p>

          <p
            v-if="!buildable"
            class="rounded-lg border p-3 text-sm"
            data-testid="contacts-share-nothing"
          >
            {{ t('contacts.share.nothing') }}
          </p>
        </template>

        <p
          v-if="error"
          class="rounded-lg border border-destructive/40 p-3 text-sm text-destructive"
          role="alert"
          data-testid="contacts-share-error"
        >
          {{ error }}
        </p>

        <div
          v-if="output"
          class="flex flex-col items-center gap-3"
          data-testid="contacts-share-result"
        >
          <img
            v-if="qrSource"
            :src="qrSource"
            :alt="t('contacts.share.qr')"
            class="size-64 rounded-lg bg-background p-2"
            data-testid="contacts-share-qr"
          />
          <p
            v-else
            class="text-center text-sm text-muted-foreground"
            data-testid="contacts-share-no-qr"
          >
            {{ t('contacts.share.noQr', { size: output.size }) }}
          </p>
          <UiButton data-testid="contacts-share-copy" @click="copyAsync">
            <Icon name="lucide:copy" class="size-4" />
            {{ t('contacts.share.copy') }}
          </UiButton>
          <code
            class="max-h-32 max-w-full overflow-y-auto break-all rounded-lg bg-muted p-2 text-xs"
            data-testid="contacts-share-link"
            >{{ output.link }}</code
          >
          <p class="text-xs text-muted-foreground">
            {{ t('contacts.share.size', { size: output.size }) }}
          </p>
        </div>
      </div>
    </template>
    <template v-if="confirming" #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="contacts-share-confirm-deny"
          @click="emit('decline')"
        >
          {{ t('contacts.share.confirm.deny') }}
        </UiButton>
        <UiButton
          type="button"
          size="sm"
          :disabled="!buildable"
          data-testid="contacts-share-confirm-allow"
          @click="allow"
        >
          {{ t('contacts.share.confirm.allow') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
