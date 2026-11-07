import { invoke } from '@tauri-apps/api/core'

/** An enrolled person (spec 066 FR-016a, `assistant_speakers_list`). */
export type EnrolledSpeaker = {
  id: string
  name: string
  utterances: number
  createdAt: number
  /** Made with another model than this build compares with: enrol again. */
  outdated: boolean
}

/** The `model-download-progress` id of the speaker model. */
export const SPEAKER_MODEL_ID = 'speaker-campplus'
/** The device setting: follow only enrolled voices (default on while someone is enrolled). */
export const ENROLLED_ONLY_KEY = 'assistant.enrolledOnly'
/** Sentences an enrolment needs (`ENROL_UTTERANCES` in assistant/speakers.rs). */
export const ENROL_SENTENCES = 3

/**
 * The people the voice assistant listens to (spec 066 FR-016a): list, enrol by reading sentences
 * (recorded with the own-voice sessions, only the voice print is kept), rename, delete.
 */
export function useAssistantSpeakers() {
  async function listAsync(): Promise<EnrolledSpeaker[]> {
    return await invoke<EnrolledSpeaker[]>('assistant_speakers_list')
  }

  async function modelAsync(): Promise<{
    installed: boolean
    sizeBytes: number
  }> {
    return await invoke('assistant_speaker_model')
  }

  async function downloadModelAsync(): Promise<void> {
    await invoke('assistant_speaker_model_download')
  }

  /** Takes recording session `sessionId` as the next sentence; returns how many there are. */
  async function addSentenceAsync(
    sessionId: string,
    generation: number,
  ): Promise<number> {
    return await invoke<number>('assistant_speaker_add_sentence', {
      sessionId,
      generation,
    })
  }

  async function resetEnrolmentAsync(): Promise<number> {
    return await invoke<number>('assistant_speaker_enrolment_reset')
  }

  async function saveAsync(name: string): Promise<EnrolledSpeaker> {
    return await invoke<EnrolledSpeaker>('assistant_speaker_save', { name })
  }

  async function renameAsync(id: string, name: string): Promise<void> {
    await invoke('assistant_speaker_rename', { id, name })
  }

  async function deleteAsync(id: string): Promise<void> {
    await invoke('assistant_speaker_delete', { id })
  }

  return {
    listAsync,
    modelAsync,
    downloadModelAsync,
    addSentenceAsync,
    resetEnrolmentAsync,
    saveAsync,
    renameAsync,
    deleteAsync,
  }
}
