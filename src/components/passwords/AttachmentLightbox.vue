<script setup lang="ts">
/**
 * The lightbox over the images of an entry (spec 036, US6, FR-038, FR-041, research R14):
 * PhotoSwipe, loaded only when it opens, with the images in card order. A slide fetches the full
 * bytes only when it is shown (and the next and previous one in advance, around the loop); a slide
 * more than one step away gives its bytes back. An image with more pixels than `MAX_PREVIEW_PIXELS`
 * is not decoded; else its size is read from the decoded image. Counter "2 von 5", the name,
 * Speichern unter, zoom by tap, pinch and wheel, arrows, arrow keys and swipe, Escape and the close
 * button come from PhotoSwipe; the focus returns to the card that opened it. An image that cannot be
 * shown leaves an empty slide and the lightbox open; why shows as a toast each time it is the shown one. Every object URL is revoked when it
 * closes, also one whose load ends after that. Only images enter; a PDF never does.
 */
import { toast } from 'vue-sonner'
import type { AttachmentView } from '@bindings/AttachmentView'
import type PhotoSwipe from 'photoswipe'
import type { SlideData } from 'photoswipe'
import { imageMime } from '~/lib/passwords/format'
import { previewableSize } from '~/lib/passwords/imageSize'

const props = defineProps<{
  /** The images of the entry in card order. */
  images: AttachmentView[]
}>()

const emit = defineEmits<{
  save: [attachment: AttachmentView]
  closed: []
}>()

const { t } = useI18n()
const { attachmentPreviewAsync } = usePasswords()
const reducedMotion = usePreferredReducedMotion()

let pswp: PhotoSwipe | null = null
let opening = false

/** The slide of an image that cannot be shown: an icon; the reason goes out as a toast. */
function failedSlide(testid: string): SlideData {
  return {
    html: `<div class="passwords-lightbox-message" data-testid="${testid}"><svg aria-hidden="true" viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m2 2 20 20"/><path d="M10.41 10.41a2 2 0 1 1-2.83-2.83"/><path d="M13.5 13.5 6 21"/><path d="M18 12l3 3"/><path d="M3.59 3.59A1.99 1.99 0 0 0 3 5v14a2 2 0 0 0 2 2h14c.55 0 1.052-.22 1.41-.59"/><path d="M21 15V5a2 2 0 0 0-2-2H9"/></svg></div>`,
  }
}

const PLACEHOLDER =
  '<div class="passwords-lightbox-message" aria-busy="true"></div>'

/** What one open lightbox holds, by slide index: the object URL of each loaded slide, the slides
 * that could not be loaded, and a token for each load still running (a load whose token was taken
 * away gives its bytes back). */
interface Session {
  instance: PhotoSwipe
  urls: Map<number, string>
  failed: Set<number>
  loading: Map<number, symbol>
  closed: boolean
}

/** The full image of one slide as an object URL with its natural size. */
async function loadSlide(
  attachment: AttachmentView,
): Promise<{ url: string; data: SlideData }> {
  const mime = imageMime(attachment.fileName)
  if (!mime) throw new Error('not an image')
  const bytes = await attachmentPreviewAsync(attachment.id)
  if (!previewableSize(bytes)) throw new Error('too many pixels')
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }))
  try {
    const image = new Image()
    image.src = url
    await image.decode()
    return {
      url,
      data: {
        src: url,
        width: image.naturalWidth,
        height: image.naturalHeight,
        alt: attachment.fileName,
      },
    }
  } catch (error) {
    URL.revokeObjectURL(url)
    throw error
  }
}

/** The index `step` slides away from `index`, around the loop. */
function wrapped(index: number, step: number, count: number): number {
  return (((index + step) % count) + count) % count
}

async function ensureSlide(session: Session, index: number) {
  if (
    session.urls.has(index) ||
    session.failed.has(index) ||
    session.loading.has(index)
  ) {
    return
  }
  const attachment = props.images[index]
  if (!attachment) return
  const token = Symbol(index)
  session.loading.set(index, token)
  const current = () => !session.closed && session.loading.get(index) === token
  let data: SlideData
  try {
    const slide = await loadSlide(attachment)
    // Closed, or moved far away, while it loaded: give the bytes back at once.
    if (!current()) {
      URL.revokeObjectURL(slide.url)
      return
    }
    session.urls.set(index, slide.url)
    data = slide.data
  } catch {
    if (!current()) return
    session.failed.add(index)
    data = failedSlide(`passwords-lightbox-error-${attachment.id}`)
  } finally {
    if (session.loading.get(index) === token) session.loading.delete(index)
  }
  ;(session.instance.options.dataSource as SlideData[])[index] = data
  session.instance.refreshSlideContent(index)
  if (index === session.instance.currIndex) tellFailure(session)
}

/**
 * The shown slide could not be loaded: says why as a toast. Called when its load fails and when the
 * lightbox moves to it, so a slide that failed in advance (a neighbour) is told when it is reached.
 */
function tellFailure(session: Session) {
  const index = session.instance.currIndex
  const attachment = props.images[index]
  if (!attachment || !session.failed.has(index)) return
  toast.error(t('passwords.lightbox.unreadable', { name: attachment.fileName }))
}

/** Loads the shown slide and its neighbours; a slide further away gives its object URL back. */
function ensureAround(session: Session) {
  const count = props.images.length
  const index = session.instance.currIndex
  const near = new Set([
    index,
    wrapped(index, 1, count),
    wrapped(index, -1, count),
  ])
  const dataSource = session.instance.options.dataSource as SlideData[]
  for (const [far, url] of session.urls) {
    if (near.has(far)) continue
    session.urls.delete(far)
    dataSource[far] = { html: PLACEHOLDER }
    session.instance.refreshSlideContent(far)
    URL.revokeObjectURL(url)
  }
  for (const far of session.loading.keys()) {
    if (!near.has(far)) session.loading.delete(far)
  }
  for (const slide of near) void ensureSlide(session, slide)
}

/** Opens the lightbox on the image at `index`. */
async function open(index: number) {
  if (pswp || opening || !props.images.length) return
  opening = true
  let PhotoSwipeClass: typeof PhotoSwipe
  try {
    ;[{ default: PhotoSwipeClass }] = await Promise.all([
      import('photoswipe'),
      import('photoswipe/style.css'),
    ])
  } finally {
    opening = false
  }
  const dataSource: SlideData[] = props.images.map(() => ({
    html: PLACEHOLDER,
  }))
  const still = reducedMotion.value === 'reduce'
  const instance = new PhotoSwipeClass({
    dataSource,
    index,
    returnFocus: true,
    bgOpacity: 0.92,
    showHideAnimationType: still ? 'none' : 'fade',
    zoomAnimationDuration: still ? false : 250,
    indexIndicatorSep: t('passwords.lightbox.of'),
    closeTitle: t('passwords.lightbox.close'),
    zoomTitle: t('passwords.lightbox.zoom'),
    arrowPrevTitle: t('passwords.lightbox.previous'),
    arrowNextTitle: t('passwords.lightbox.next'),
    errorMsg: t('passwords.lightbox.unreadableShort'),
    mainClass: 'passwords-lightbox',
  })
  pswp = instance
  const session: Session = {
    instance,
    urls: new Map(),
    failed: new Set(),
    loading: new Map(),
    closed: false,
  }
  instance.on('uiRegister', () => {
    instance.ui?.registerElement({
      name: 'save',
      order: 8,
      isButton: true,
      title: t('passwords.attachments.download'),
      html: '<svg aria-hidden="true" class="pswp__icn" style="fill: none; color: var(--pswp-icon-color)" viewBox="-4 -4 32 32" width="32" height="32" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/></svg>',
      onInit: (element) => {
        element.setAttribute('data-testid', 'passwords-lightbox-save')
      },
      onClick: () => {
        const attachment = props.images[instance.currIndex]
        if (attachment) emit('save', attachment)
      },
    })
    instance.ui?.registerElement({
      name: 'name',
      order: 9,
      isButton: false,
      appendTo: 'root',
      onInit: (element) => {
        element.classList.add('passwords-lightbox-name')
        element.setAttribute('data-testid', 'passwords-lightbox-name')
        const show = () => {
          element.textContent = props.images[instance.currIndex]?.fileName ?? ''
        }
        instance.on('change', show)
        show()
      },
    })
  })
  instance.on('afterInit', () => ensureAround(session))
  instance.on('change', () => {
    ensureAround(session)
    tellFailure(session)
  })
  instance.on('destroy', () => {
    session.closed = true
    for (const url of session.urls.values()) URL.revokeObjectURL(url)
    session.urls.clear()
    session.loading.clear()
    if (pswp === instance) pswp = null
    emit('closed')
  })
  instance.init()
}

onBeforeUnmount(() => pswp?.destroy())

defineExpose({ open })
</script>

<template>
  <span hidden />
</template>

<style>
.passwords-lightbox .passwords-lightbox-message {
  display: flex;
  height: 100%;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  color: #fff;
  text-align: center;
}
.passwords-lightbox .passwords-lightbox-name {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  padding: 0.75rem 1rem;
  color: #fff;
  text-align: center;
  font-size: 0.875rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  background: linear-gradient(transparent, rgb(0 0 0 / 0.6));
}
</style>
