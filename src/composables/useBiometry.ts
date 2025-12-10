import { ref } from 'vue'
import {
  checkStatus as biometryCheckStatus,
  authenticate as biometryAuthenticate,
  hasData as biometryHasData,
  getData as biometryGetData,
  setData as biometrySetData,
  removeData as biometryRemoveData,
  BiometryType,
  type Status,
  type AuthOptions,
  type DataOptions,
  type GetDataOptions,
  type SetDataOptions,
} from '@choochmeque/tauri-plugin-biometry-api'

export { BiometryType }

export function useBiometry() {
  const status = ref<Status | null>(null)
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  async function checkStatus(): Promise<Status> {
    try {
      isLoading.value = true
      error.value = null
      status.value = await biometryCheckStatus()
      return status.value
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      throw e
    } finally {
      isLoading.value = false
    }
  }

  async function authenticate(
    reason: string,
    options?: AuthOptions,
  ): Promise<boolean> {
    try {
      isLoading.value = true
      error.value = null
      await biometryAuthenticate(reason, options)
      return true
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      return false
    } finally {
      isLoading.value = false
    }
  }

  async function hasData(options: DataOptions): Promise<boolean> {
    try {
      return await biometryHasData(options)
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      return false
    }
  }

  async function getData(options: GetDataOptions): Promise<string | null> {
    try {
      isLoading.value = true
      error.value = null
      const response = await biometryGetData(options)
      return response.data
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      return null
    } finally {
      isLoading.value = false
    }
  }

  async function setData(options: SetDataOptions): Promise<boolean> {
    try {
      isLoading.value = true
      error.value = null
      await biometrySetData(options)
      return true
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      return false
    } finally {
      isLoading.value = false
    }
  }

  async function removeData(options: DataOptions): Promise<boolean> {
    try {
      await biometryRemoveData(options)
      return true
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      return false
    }
  }

  function getBiometryTypeName(): string | null {
    if (!status.value) return null
    switch (status.value.biometryType) {
      case BiometryType.TouchID:
        return 'Fingerprint'
      case BiometryType.FaceID:
        return 'Face'
      case BiometryType.Iris:
        return 'Iris'
      case BiometryType.Auto:
        return 'Biometric'
      default:
        return null
    }
  }

  return {
    status,
    isLoading,
    error,
    checkStatus,
    authenticate,
    hasData,
    getData,
    setData,
    removeData,
    getBiometryTypeName,
  }
}
