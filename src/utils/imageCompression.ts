const AVATAR_SIZE = 128
const WEBP_QUALITY = 0.8

/**
 * Compresses an image file to a 128x128 WebP Base64 string.
 * Uses Canvas API for resizing and format conversion.
 */
export async function compressImageToBase64(file: File | Blob): Promise<string> {
  const bitmap = await createImageBitmap(file)
  const canvas = new OffscreenCanvas(AVATAR_SIZE, AVATAR_SIZE)
  const ctx = canvas.getContext('2d')!

  // Draw image centered and cropped to square
  const size = Math.min(bitmap.width, bitmap.height)
  const sx = (bitmap.width - size) / 2
  const sy = (bitmap.height - size) / 2
  ctx.drawImage(bitmap, sx, sy, size, size, 0, 0, AVATAR_SIZE, AVATAR_SIZE)
  bitmap.close()

  const blob = await canvas.convertToBlob({ type: 'image/webp', quality: WEBP_QUALITY })
  return blobToBase64(blob)
}

/**
 * Compresses image data from a canvas (e.g. from cropper) to Base64 WebP.
 */
export async function compressCanvasToBase64(canvas: HTMLCanvasElement): Promise<string> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (!blob) return reject(new Error('Canvas toBlob failed'))
        resolve(blobToBase64(blob))
      },
      'image/webp',
      WEBP_QUALITY,
    )
  })
}

/**
 * Renders an SVG string to a 128x128 WebP Base64 string.
 */
export async function compressSvgToBase64(svgString: string): Promise<string> {
  const blob = new Blob([svgString], { type: 'image/svg+xml;charset=utf-8' })
  const url = URL.createObjectURL(blob)

  try {
    const img = new Image()
    img.width = AVATAR_SIZE
    img.height = AVATAR_SIZE
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve()
      img.onerror = reject
      img.src = url
    })

    const canvas = new OffscreenCanvas(AVATAR_SIZE, AVATAR_SIZE)
    const ctx = canvas.getContext('2d')!
    ctx.drawImage(img, 0, 0, AVATAR_SIZE, AVATAR_SIZE)

    const webpBlob = await canvas.convertToBlob({ type: 'image/webp', quality: WEBP_QUALITY })
    return blobToBase64(webpBlob)
  } finally {
    URL.revokeObjectURL(url)
  }
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = reject
    reader.readAsDataURL(blob)
  })
}
