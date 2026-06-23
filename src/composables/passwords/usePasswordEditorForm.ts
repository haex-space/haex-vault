import * as OTPAuth from 'otpauth'
import { useClipboard } from '@vueuse/core'
import type { AttachmentWithSize } from '~/types/passwords/attachment'

export type EditableKeyValue = { id: string; key: string; value: string }

export const otpAlgorithms = ['SHA1', 'SHA256', 'SHA512'] as const
export type OtpAlgorithm = (typeof otpAlgorithms)[number]

export type EditorForm = {
  title: string
  username: string
  password: string
  url: string
  note: string
  icon: string
  color: string
  expiresAt: string
  otpSecret: string
  otpDigits: number
  otpPeriod: number
  otpAlgorithm: OtpAlgorithm
  tagNames: string[]
  keyValues: EditableKeyValue[]
  autofillAliases: Record<string, string[]>
}

export const usePasswordEditorForm = () => {
  const passwordsStore = usePasswordsStore()
  const { selectedItem, selectedItemTags, isEditing } =
    storeToRefs(passwordsStore)

  const isCreating = computed(() => !selectedItem.value)

  const form = reactive<EditorForm>({
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
      'SHA1') as OtpAlgorithm,
    tagNames: selectedItemTags.value.map((t) => t.name),
    keyValues: [] as EditableKeyValue[],
    autofillAliases: JSON.parse(
      JSON.stringify(selectedItem.value?.autofillAliases ?? {}),
    ) as Record<string, string[]>,
  })

  // Snapshot of the pristine form for cancel-from-edit on existing items.
  // Reactive so isDirty recomputes when snapshot changes (save, load, revert).
  const formSnapshot = reactive(JSON.parse(JSON.stringify(form)) as EditorForm)

  const errors = reactive({
    title: [] as string[],
    tags: [] as string[],
  })

  const attachments = ref<AttachmentWithSize[]>([])
  const attachmentsSnapshot = ref<AttachmentWithSize[]>([])
  const attachmentsToAdd = ref<AttachmentWithSize[]>([])
  const attachmentsToDelete = ref<AttachmentWithSize[]>([])

  const isDirty = computed(
    () =>
      JSON.stringify(form) !== JSON.stringify(formSnapshot) ||
      JSON.stringify(attachments.value.map(({ id, fileName }) => ({ id, fileName }))) !==
        JSON.stringify(attachmentsSnapshot.value.map(({ id, fileName }) => ({ id, fileName }))) ||
      attachmentsToAdd.value.length > 0 ||
      attachmentsToDelete.value.length > 0,
  )

  // Master-detail state for key-value extra tab
  const currentSelectedKv = ref<EditableKeyValue | null>(null)

  const visibleKeyValues = computed(() =>
    isEditing.value
      ? form.keyValues
      : form.keyValues.filter((kv) => kv.key.trim() || kv.value.trim()),
  )

  const currentKvValue = computed({
    get: () => currentSelectedKv.value?.value ?? '',
    set(newValue: string) {
      if (currentSelectedKv.value) currentSelectedKv.value.value = newValue
    },
  })

  const { copy: copyKv } = useClipboard({ copiedDuring: 2000 })
  const kvCopiedItem = ref<EditableKeyValue | null>(null)

  const copyKvValue = async (kv: EditableKeyValue) => {
    if (!kv.value) return
    await copyKv(kv.value)
    kvCopiedItem.value = kv
    setTimeout(() => { kvCopiedItem.value = null }, 2000)
  }

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

  let _kvAdding = false
  const addKeyValue = async (
    focusEl?: { $el?: HTMLElement } | null,
  ) => {
    if (_kvAdding) return
    _kvAdding = true
    const newKv = { id: crypto.randomUUID(), key: '', value: '' }
    form.keyValues.push(newKv)
    currentSelectedKv.value = newKv
    await nextTick()
    focusEl?.$el?.querySelector('input')?.focus()
    _kvAdding = false
  }

  const removeKeyValue = (index: number) => {
    const removed = form.keyValues[index]
    form.keyValues.splice(index, 1)
    if (currentSelectedKv.value?.id === removed?.id) {
      currentSelectedKv.value = form.keyValues[0] ?? null
    }
  }

  const revertForm = () => {
    Object.assign(form, JSON.parse(JSON.stringify(formSnapshot)))
    attachments.value = JSON.parse(JSON.stringify(attachmentsSnapshot.value))
    attachmentsToAdd.value = []
    attachmentsToDelete.value = []
    errors.title = []
    errors.tags = []
    currentSelectedKv.value = form.keyValues[0] ?? null
  }

  return {
    // store-derived
    selectedItem,
    isEditing,
    isCreating,
    // form state
    form,
    formSnapshot,
    errors,
    // attachments
    attachments,
    attachmentsSnapshot,
    attachmentsToAdd,
    attachmentsToDelete,
    // dirty
    isDirty,
    // key-values
    currentSelectedKv,
    visibleKeyValues,
    currentKvValue,
    kvCopiedItem,
    copyKvValue,
    addKeyValue,
    removeKeyValue,
    // expiry
    isExpired,
    // otp
    otpCode,
    otpFormatted,
    otpRemaining,
    otpDashArray,
    copiedOtp,
    copyOtp,
    // misc
    revertForm,
  }
}
