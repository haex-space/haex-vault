import { addBinaryAsync, arrayBufferToBase64 } from '~/utils/passwords/binaries'

export const useFavicon = () => {
  const downloadFaviconAsync = async (url: string): Promise<string | null> => {
    let domain: string
    try {
      domain = new URL(url).hostname
    } catch {
      console.error('[Favicon] Invalid URL format:', url)
      return null
    }

    const faviconUrl = `https://icons.duckduckgo.com/ip3/${domain}.ico`

    try {
      const response = await fetch(faviconUrl)
      if (!response.ok) return null

      const buffer = await response.arrayBuffer()
      const base64 = arrayBufferToBase64(buffer)
      const hash = await addBinaryAsync(base64, buffer.byteLength, 'icon')
      return `binary:${hash}`
    } catch (error) {
      console.error('[Favicon] Failed to fetch favicon:', error)
      return null
    }
  }

  return {
    downloadFaviconAsync,
  }
}
