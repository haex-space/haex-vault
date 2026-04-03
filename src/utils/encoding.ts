export function toBase64(data: Uint8Array): string {
  return btoa(String.fromCharCode(...data))
}

export function fromBase64(b64: string): Uint8Array {
  const binary = atob(b64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return bytes
}

export function toBase64Url(data: Uint8Array): string {
  return toBase64(data).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}
