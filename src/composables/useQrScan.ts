import { formatFromPlugin, type FoundCode } from '~/lib/wallet/capture'

/**
 * Scanning a QR code with the camera (spec 085, research R3, R4): on a phone the official barcode
 * plugin, which asks for the camera permission itself at the moment of the tap. The text it reads
 * goes to the caller as is; what it means is for the caller (the backend reads link texts and
 * contact shares). A cancelled scan or a denied permission rejects, and the caller shows it as a
 * toast. A desktop has no such plugin: it shows a live preview of its camera in the sheet that
 * wants the scan (`LinkScanner`), which `usesPreview` tells. The camera of a pass reads the five
 * formats a pass keeps (spec 092, research R15) and returns text and format (`scanCodeAsync`).
 */
export function useQrScan() {
  const { capabilities } = useDeviceCapabilities()

  /** Whether the plugin scans on this device: phones and tablets (Android, iOS). */
  const canScan = computed(
    () =>
      capabilities.value?.platform === 'android' ||
      capabilities.value?.platform === 'ios',
  )
  /** Whether a scan here is a preview of the camera inside the sheet: the desktops. */
  const usesPreview = computed(() => !canScan.value)

  async function scanAsync(): Promise<string> {
    const { scan, Format } = await import('@tauri-apps/plugin-barcode-scanner')
    const scanned = await scan({ formats: [Format.QRCode] })
    return scanned.content
  }

  /** One code of a pass: QR, Code 128, EAN-13, PDF417 or Aztec, with its stored format. */
  async function scanCodeAsync(): Promise<FoundCode> {
    const { scan, Format } = await import('@tauri-apps/plugin-barcode-scanner')
    const scanned = await scan({
      formats: [
        Format.QRCode,
        Format.Code128,
        Format.EAN13,
        Format.PDF417,
        Format.Aztec,
      ],
    })
    const format = formatFromPlugin(scanned.format)
    if (format === null) throw new Error('unsupported_format')
    return { format, value: scanned.content }
  }

  return { canScan, usesPreview, scanAsync, scanCodeAsync }
}
