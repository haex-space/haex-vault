<script setup lang="ts">
/**
 * Reads a code for a pass from a photo or a PDF (spec 092, FR-034, research R15). The person
 * chooses the source, sees what was found (format and value, on their own screen), picks when
 * there are several, and says where it goes: a new pass or one the vault has. Only that confirmation
 * writes anything; a value is saved as text and never opened, called or interpreted. The camera is
 * the live preview of spec 085 on a desktop and the barcode plugin on a phone; where it is no use
 * it says so in a toast and photo, PDF and typing remain (typing is the editor of a new pass).
 */
import { toast } from 'vue-sonner'
import type { PickedFile } from '@bindings/PickedFile'
import { displayTitle } from '~/lib/passwords/format'
import { whenText } from '~/lib/passwords/passDate'
import { PASS_KINDS, entryTypeOf } from '~/lib/passwords/passType'
import { filterHeaders } from '~/lib/passwords/search'
import { isInTrash, trashGroupIds } from '~/lib/passwords/tree'
import {
  addFound,
  newPassCodes,
  pageStep,
  withPicked,
  type FoundCode,
} from '~/lib/wallet/capture'
import type { PdfSession } from '~/lib/wallet/capturePdf'
import { walletNewPassLocation } from '~/lib/wallet/registry'
import { releaseSharedFile } from '~/lib/wallet/sharedFiles'

const open = defineModel<boolean>('open', { required: true })

const { t, locale } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const store = usePasswordsStore()
const { pickOneAsync } = usePickedFile()
const { canScan, scanCodeAsync } = useQrScan()
const { tabId, windowId } = useWmTab()
const wm = useWindowManagerStore()
const { scanFileAsync, scanImageAsync, pdfBytesAsync } = usePassCapture()
const { createPassAsync, updatePassAsync, codesAsync } = usePasses()
const shared = useWalletCaptureStore()

type Step = 'choose' | 'camera' | 'scanning' | 'pick' | 'target'
const step = ref<Step>('choose')
const found = ref<FoundCode[]>([])
const picked = ref<boolean[]>([])
const nothingFound = ref(false)
const saving = ref(false)

const target = ref<'new' | 'existing'>('new')
const title = ref('')
const passKind = ref('other')
const existingId = ref<string | null>(null)
const query = ref('')

/** The PDF being read and the page the next step starts at. */
let pdf: PdfSession | null = null
const nextPage = ref(0)
const morePages = ref(false)
/** A new run (or a close) makes a slow read of an older one stop. */
let run = 0

const KIND_OPTIONS = PASS_KINDS.map((kind) => ({
  value: kind,
  label: t(`passwords.kinds.${kind}`),
}))
const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone

function reset() {
  run += 1
  pdf?.destroy()
  pdf = null
  step.value = 'choose'
  found.value = []
  picked.value = []
  nothingFound.value = false
  target.value = 'new'
  title.value = ''
  passKind.value = 'other'
  existingId.value = null
  query.value = ''
  nextPage.value = 0
  morePages.value = false
}

watch(
  open,
  (isOpen) => {
    if (isOpen) {
      reset()
      // A file another app shared (FR-037) is read at once, as if chosen here.
      const next = shared.take()
      if (next !== null) {
        void (next.kind === 'pdf'
          ? readPdfAsync(next.file)
          : readPhotoAsync(next.file))
      }
    } else {
      run += 1
      pdf?.destroy()
      pdf = null
      // The preview goes with the dialog, whether or not the drawer keeps its content mounted.
      step.value = 'choose'
    }
  },
  { immediate: true },
)

/**
 * Tabs stay mounted behind `v-show` and a window can be minimized: the preview must not keep the
 * camera on while its tab or window is out of sight, so the step falls back to the choice.
 */
const shown = computed(() => {
  const window = wm.windows.find((candidate) => candidate.id === windowId)
  return !window || (!window.minimized && window.activeTabId === tabId)
})
watch(shown, (visible) => {
  if (!visible && step.value === 'camera') step.value = 'choose'
})
onBeforeUnmount(() => {
  // A load still pending sees a new run and drops its own session.
  run += 1
  pdf?.destroy()
  pdf = null
})

/** Takes what a read found: one code is picked already, several are the person's to pick. */
function take(codes: FoundCode[]) {
  const before = found.value.length
  found.value = addFound(found.value, codes)
  const single = found.value.length === 1
  picked.value = found.value.map((_, i) =>
    i < before ? (picked.value[i] ?? false) : single,
  )
  nothingFound.value = found.value.length === 0
  step.value = 'pick'
}

/** The camera: the plugin of a phone asks for one code at once, a desktop shows its preview. */
async function fromCameraAsync() {
  if (!canScan.value) {
    step.value = 'camera'
    return
  }
  const mine = ++run
  try {
    const code = await scanCodeAsync()
    if (mine === run) take([code])
  } catch (cause) {
    if (mine === run) toast.error(errString(cause))
  }
}

/** The preview read codes: they are the person's to pick, and the camera is given back. */
async function acceptCodesAsync(codes: FoundCode[]): Promise<boolean> {
  take(codes)
  return true
}

async function fromPhotoAsync() {
  const file = await pickOneAsync([
    {
      name: t('wallet.capture.photoFilter'),
      extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'],
    },
  ])
  if (file === null) return
  await readPhotoAsync(file)
}

async function readPhotoAsync(file: PickedFile) {
  const mine = ++run
  step.value = 'scanning'
  try {
    const codes = await scanFileAsync(file)
    if (mine === run) take(codes)
  } catch (cause) {
    if (mine !== run) return
    step.value = 'choose'
    toast.error(errString(cause))
  } finally {
    // Read or failed: a copy of a shared file goes (FR-035).
    void releaseSharedFile(file)
  }
}

async function fromPdfAsync() {
  const file = await pickOneAsync([
    { name: t('wallet.capture.pdfFilter'), extensions: ['pdf'] },
  ])
  if (file === null) return
  await readPdfAsync(file)
}

async function readPdfAsync(file: PickedFile) {
  const mine = ++run
  step.value = 'scanning'
  try {
    const bytes = await pdfBytesAsync(file)
    // The bytes are in the window now: a copy of a shared file goes (FR-035).
    void releaseSharedFile(file)
    const { openPdfAsync } = await import('~/lib/wallet/capturePdf')
    const session = await openPdfAsync(bytes)
    if (mine !== run) {
      session.destroy()
      return
    }
    pdf?.destroy()
    pdf = session
    nextPage.value = 0
    await readPagesAsync(mine)
  } catch (cause) {
    void releaseSharedFile(file)
    if (mine !== run) return
    step.value = 'choose'
    toast.error(errString(cause))
  }
}

/** Reads the next step of pages, one at a time. */
async function readPagesAsync(mine: number) {
  const session = pdf
  if (session === null) return
  const { pages, more } = pageStep(session.pages, nextPage.value)
  let all: FoundCode[] = []
  for (const number of pages) {
    const image = await session.pageImageAsync(number)
    if (mine !== run) return
    if (image === null) continue
    all = addFound(all, await scanImageAsync(image))
    if (mine !== run) return
  }
  nextPage.value += pages.length
  morePages.value = more
  take(all)
}

async function morePagesAsync() {
  const mine = ++run
  step.value = 'scanning'
  try {
    await readPagesAsync(mine)
  } catch (cause) {
    if (mine !== run) return
    step.value = 'pick'
    toast.error(errString(cause))
  }
}

function typeByHand() {
  open.value = false
  void router.push(walletNewPassLocation())
}

const chosen = computed(() => found.value.filter((_, i) => picked.value[i]))

/** The passes the vault has, searched like the list. */
const passes = computed(() => {
  const trash = trashGroupIds(store.groups)
  const live = store.headers.filter(
    (header) =>
      entryTypeOf(header) === 'pass' && !isInTrash(header.groupId, trash),
  )
  return filterHeaders(live, { query: query.value }).slice(0, 50)
})

const canSave = computed(
  () =>
    !saving.value &&
    chosen.value.length > 0 &&
    (target.value === 'new' || existingId.value !== null),
)

async function saveAsync() {
  if (!canSave.value) return
  // What the person confirmed, fixed before anything is awaited.
  const confirmed = [...chosen.value]
  saving.value = true
  try {
    let id: string
    if (target.value === 'new') {
      const created = await createPassAsync({
        title: title.value.trim() || undefined,
        passKind: passKind.value,
        codes: newPassCodes(confirmed),
        keyValues: [],
        tagNames: [],
      })
      id = created.id
    } else {
      id = existingId.value as string
      const header = store.headersById.get(id)
      const stored = await codesAsync(id)
      await updatePassAsync(id, header?.updatedAt ?? '', {
        codes: withPicked(
          stored.map((code) => ({
            id: code.id,
            format: code.format,
            value: code.value,
            label: code.label ?? undefined,
            position: code.position,
          })),
          confirmed,
        ),
      })
    }
    await store.quietReloadAsync()
    open.value = false
    void router.push(`/entry/${id}`)
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    saving.value = false
  }
}

function describe(header: (typeof passes.value)[number]): string | undefined {
  return whenText(header, deviceZone, locale.value) ?? undefined
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('wallet.capture.title')">
    <template #content>
      <div class="flex flex-col gap-4" data-testid="wallet-capture-dialog">
        <template v-if="step === 'choose'">
          <p class="text-sm text-muted-foreground">
            {{ t('wallet.capture.intro') }}
          </p>
          <SettingsGroup>
            <SettingsRow
              :title="t('wallet.capture.useCamera')"
              :description="t('wallet.capture.useCameraHint')"
              icon="lucide:scan-line"
              navigates
              data-testid="wallet-capture-camera"
              @click="fromCameraAsync"
            />
            <SettingsRow
              :title="t('wallet.capture.photo')"
              :description="t('wallet.capture.photoHint')"
              icon="lucide:image"
              navigates
              data-testid="wallet-capture-photo"
              @click="fromPhotoAsync"
            />
            <SettingsRow
              :title="t('wallet.capture.pdf')"
              :description="t('wallet.capture.pdfHint')"
              icon="lucide:file-text"
              navigates
              data-testid="wallet-capture-pdf"
              @click="fromPdfAsync"
            />
            <SettingsRow
              :title="t('wallet.capture.type')"
              :description="t('wallet.capture.typeHint')"
              icon="lucide:keyboard"
              navigates
              data-testid="wallet-capture-type"
              @click="typeByHand"
            />
          </SettingsGroup>
        </template>

        <OnboardingLinkScanner
          v-else-if="step === 'camera'"
          :accept-codes="acceptCodesAsync"
          @close="step = 'choose'"
        />

        <p
          v-else-if="step === 'scanning'"
          class="py-6 text-center text-muted-foreground"
          data-testid="wallet-capture-scanning"
        >
          {{ t('wallet.capture.scanning') }}
        </p>

        <template v-else-if="step === 'pick'">
          <p
            v-if="nothingFound"
            class="py-4 text-center text-muted-foreground"
            data-testid="wallet-capture-none"
          >
            {{ t('wallet.capture.none') }}
          </p>
          <template v-else>
            <p class="text-sm text-muted-foreground">
              {{
                found.length > 1
                  ? t('wallet.capture.pickSeveral')
                  : t('wallet.capture.pickOne')
              }}
            </p>
            <SettingsGroup>
              <li
                v-for="(code, index) in found"
                :key="`${code.format}:${code.value}`"
                class="flex items-center gap-3 px-4 py-3"
                :data-testid="`wallet-capture-code-${index}`"
              >
                <ShadcnCheckbox
                  :id="`wallet-capture-pick-${index}`"
                  :model-value="picked[index] === true"
                  :data-testid="`wallet-capture-pick-${index}`"
                  @update:model-value="picked[index] = $event === true"
                />
                <label
                  :for="`wallet-capture-pick-${index}`"
                  class="flex min-w-0 flex-1 flex-col"
                >
                  <span class="text-xs text-muted-foreground">{{
                    code.format
                  }}</span>
                  <span class="break-all text-sm">{{ code.value }}</span>
                </label>
              </li>
            </SettingsGroup>
          </template>
          <div class="flex flex-wrap justify-end gap-2">
            <UiButton
              v-if="morePages"
              type="button"
              variant="outline"
              data-testid="wallet-capture-more"
              @click="morePagesAsync"
            >
              {{ t('wallet.capture.morePages') }}
            </UiButton>
            <UiButton
              type="button"
              variant="outline"
              data-testid="wallet-capture-back"
              @click="reset"
            >
              {{ t('wallet.capture.again') }}
            </UiButton>
            <UiButton
              v-if="!nothingFound"
              type="button"
              :disabled="chosen.length === 0"
              data-testid="wallet-capture-next"
              @click="step = 'target'"
            >
              {{ t('wallet.capture.next') }}
            </UiButton>
          </div>
        </template>

        <template v-else>
          <ShadcnRadioGroup v-model="target" class="flex flex-col gap-3">
            <label class="flex items-center gap-2 text-sm">
              <ShadcnRadioGroupItem
                id="wallet-capture-target-new"
                value="new"
                data-testid="wallet-capture-target-new"
              />
              {{ t('wallet.capture.toNew') }}
            </label>
            <label class="flex items-center gap-2 text-sm">
              <ShadcnRadioGroupItem
                id="wallet-capture-target-existing"
                value="existing"
                data-testid="wallet-capture-target-existing"
              />
              {{ t('wallet.capture.toExisting') }}
            </label>
          </ShadcnRadioGroup>

          <SettingsGroup v-if="target === 'new'">
            <li class="px-4 py-3">
              <UiInput
                id="wallet-capture-title"
                v-model="title"
                :label="t('passwords.fields.title')"
                :labels="fieldLabels.input.value"
                label-bg="var(--muted)"
                data-testid="wallet-capture-title"
              />
            </li>
            <li class="px-4 py-3">
              <UiSelect
                id="wallet-capture-kind"
                v-model="passKind"
                :options="KIND_OPTIONS"
                :label="t('wallet.capture.kindAsk')"
                label-bg="var(--muted)"
                data-testid="wallet-capture-kind"
              />
              <p class="px-1 pt-1 text-xs text-muted-foreground">
                {{ t('wallet.capture.placeHint') }}
              </p>
            </li>
          </SettingsGroup>
          <template v-else>
            <UiInput
              v-model="query"
              type="search"
              :label="t('passwords.references.searchEntry')"
              :labels="fieldLabels.input.value"
              data-testid="wallet-capture-search"
            />
            <p
              v-if="passes.length === 0"
              class="py-2 text-center text-sm text-muted-foreground"
            >
              {{ t('passwords.noMatches') }}
            </p>
            <SettingsGroup v-else>
              <SettingsRow
                v-for="header in passes"
                :key="header.id"
                :title="displayTitle(header.title) ?? t('passwords.untitled')"
                :description="
                  existingId === header.id
                    ? t('wallet.capture.chosen')
                    : describe(header)
                "
                navigates
                :data-testid="`wallet-capture-existing-${header.id}`"
                @click="existingId = header.id"
              />
            </SettingsGroup>
          </template>

          <div class="flex flex-wrap justify-end gap-2">
            <UiButton
              type="button"
              variant="outline"
              :disabled="saving"
              @click="step = 'pick'"
            >
              {{ t('wallet.back') }}
            </UiButton>
            <UiButton
              type="button"
              :loading="saving"
              :disabled="!canSave"
              data-testid="wallet-capture-save"
              @click="saveAsync"
            >
              {{ t('passwords.save') }}
            </UiButton>
          </div>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>
