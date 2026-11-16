import { formatFromPlugin, type FoundCode } from '~/lib/wallet/capture'

/**
 * Scanning a QR code with the camera (spec 085, research R3, R4): on a phone the official barcode
 * plugin; the camera permission is asked for here at the moment of the tap. The text it reads
 * goes to the caller as is; what it means is for the caller (the backend reads link texts and
 * contact shares). A denied permission rejects, and the caller shows it as a toast; a scan the
 * person ended (close button, back gesture) rejects with `ScanCancelledError`, which the caller
 * leaves without a message (`isScanCancelled`). A desktop has no such plugin: it shows a live preview of its camera in the sheet that
 * wants the scan (`LinkScanner`), which `usesPreview` tells. The camera of a pass reads the five
 * formats a pass keeps (spec 092, research R15) and returns text and format (`scanCodeAsync`).
 */
export function useQrScan() {
  const { capabilities } = useDeviceCapabilities()
  const { t } = useI18n()
  const session = useScanSession()

  /** Whether the plugin scans on this device: phones and tablets (Android, iOS). */
  const canScan = computed(
    () =>
      capabilities.value?.platform === 'android' ||
      capabilities.value?.platform === 'ios',
  )
  /** Whether a scan here is a preview of the camera inside the sheet: the desktops. */
  const usesPreview = computed(() => !canScan.value)

  /**
   * The plugin does NOT ask for the camera itself: on Android its `scan` throws "No permission to
   * use camera" unless the permission was granted before. So the question is asked here, at the
   * moment of the tap; a refusal is a message that says where to allow it.
   */
  async function ensureCameraAsync(): Promise<void> {
    const { checkPermissions, requestPermissions } =
      await import('@tauri-apps/plugin-barcode-scanner')
    let state = await checkPermissions()
    if (state !== 'granted') state = await requestPermissions()
    if (state !== 'granted') throw new Error(t('errors.cameraDenied'))
  }

  async function scanAsync(): Promise<string> {
    await ensureCameraAsync()
    const { scan, Format } = await import('@tauri-apps/plugin-barcode-scanner')
    // Windowed: the page lies transparent above the camera, so `ScanOverlay` can show a close button.
    const scanned = await session.runAsync(() =>
      scan({ formats: [Format.QRCode], windowed: true }),
    )
    return scanned.content
  }

  /** One code of a pass: QR, Code 128, EAN-13, PDF417 or Aztec, with its stored format. */
  async function scanCodeAsync(): Promise<FoundCode> {
    await ensureCameraAsync()
    const { scan, Format } = await import('@tauri-apps/plugin-barcode-scanner')
    const scanned = await session.runAsync(() =>
      scan({
        formats: [
          Format.QRCode,
          Format.Code128,
          Format.EAN13,
          Format.PDF417,
          Format.Aztec,
        ],
        windowed: true,
      }),
    )
    const format = formatFromPlugin(scanned.format)
    if (format === null) throw new Error('unsupported_format')
    return { format, value: scanned.content }
  }

  return { canScan, usesPreview, scanAsync, scanCodeAsync }
}
