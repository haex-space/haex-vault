import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { translations } from '../lib/translations.ts'
import { createAndUnlock, openChat } from '../lib/flows.ts'
import { ERROR_TOAST } from '../lib/toasts.ts'

// Spec 043 FR-029 (quickstart §8 step 3): the speech input asks Android for the microphone before
// the first recording; when the person refused it, an error toast explains how to allow it later and
// nothing is recorded. The system's own question cannot be driven, so the refusal is given
// beforehand through the phone's controls, as after a second "Don't allow".

/** The explanation in every language holzi speaks. */
const EXPLANATIONS = translations('voiceControl.error.microphoneDenied')

scenario('android-microphone', { needs: { phone: true } }, async (ctx) => {
  const instance = await ctx.startInstance()
  const phone = instance.phone
  assert.ok(phone !== undefined, 'a phone has its controls')
  await createAndUnlock(instance, { name: 'phone-vault' })
  phone.refuseMicrophone()

  await openChat(instance)
  await instance.click('voice-start')
  await instance.waitForDisplayed(ERROR_TOAST)
  const shown = await instance.exec<string>(
    `return document.querySelector(arguments[0])?.textContent?.trim() ?? ''`,
    [ERROR_TOAST],
  )
  assert.ok(
    EXPLANATIONS.some((explanation) => shown.includes(explanation)),
    `the error toast says "${shown}"`,
  )

  const answer = await instance.invoke('start_voice_recording')
  assert.ok('ok' in answer && !answer.ok, 'a recording started')
  assert.equal((answer.error as { kind?: string }).kind, 'MicrophoneDenied')
  ctx.step('a refused microphone is explained and records nothing')
})
