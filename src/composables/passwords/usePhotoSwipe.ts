import PhotoSwipeLightbox from 'photoswipe/lightbox'
import 'photoswipe/style.css'

export interface PhotoSwipeItem {
  src: string
  width: number
  height: number
  alt?: string
}

async function resolveDimensions(src: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve) => {
    const img = new Image()
    img.onload = () => resolve({ width: img.naturalWidth, height: img.naturalHeight })
    img.onerror = () => resolve({ width: 1920, height: 1080 })
    img.src = src
  })
}

/**
 * Opens a PhotoSwipe lightbox for a list of image sources.
 * `startIndex` sets which image is shown first.
 */
export async function openPhotoSwipe(
  items: Array<{ src: string; alt?: string }>,
  startIndex = 0,
): Promise<void> {
  const resolved: PhotoSwipeItem[] = await Promise.all(
    items.map(async (item) => {
      const dimensions = await resolveDimensions(item.src)
      return { ...dimensions, src: item.src, alt: item.alt }
    }),
  )

  const lightbox = new PhotoSwipeLightbox({
    dataSource: resolved,
    pswpModule: () => import('photoswipe'),
    showHideAnimationType: 'zoom',
    preload: [1, 2],
  })

  lightbox.init()
  lightbox.loadAndOpen(startIndex)
}
