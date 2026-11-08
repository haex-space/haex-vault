/**
 * Scanning a QR code with the camera (spec 085, research R3): on a phone the official barcode
 * plugin, which asks for the camera permission itself at the moment of the tap. The text it reads
 * goes to the caller as is; what it means is for the caller (the backend reads link texts and
 * contact shares). A cancelled scan or a denied permission rejects, and the caller shows it as a
 * toast.
 */
export function useQrScan() {
  const { isAndroid } = useDeviceCapabilities()

  /** Whether this device has a scanner. */
  const canScan = computed(() => isAndroid.value)

  async function scanAsync(): Promise<string> {
    const { scan, Format } = await import('@tauri-apps/plugin-barcode-scanner')
    const scanned = await scan({ formats: [Format.QRCode] })
    return scanned.content
  }

  return { canScan, scanAsync }
}
