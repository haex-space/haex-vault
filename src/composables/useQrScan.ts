/**
 * Scanning a QR code with the camera (spec 085, research R3, R4): on a phone the official barcode
 * plugin, which asks for the camera permission itself at the moment of the tap. The text it reads
 * goes to the caller as is; what it means is for the caller (the backend reads link texts and
 * contact shares). A cancelled scan or a denied permission rejects, and the caller shows it as a
 * toast. A desktop has no such plugin: it shows a live preview of its camera in the sheet that
 * wants the scan (`LinkScanner`), which `usesPreview` tells.
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

  return { canScan, usesPreview, scanAsync }
}
