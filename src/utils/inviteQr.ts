import QRCode from 'qrcode'

/**
 * Renders an invite link onto a canvas using the project-wide QR style
 * (200px, thin margin, strict B/W for maximum scanner compatibility).
 */
export async function renderInviteQrAsync(
  canvas: HTMLCanvasElement,
  link: string,
): Promise<void> {
  await QRCode.toCanvas(canvas, link, {
    width: 200,
    margin: 1,
    color: { dark: '#000000', light: '#ffffff' },
  })
}
