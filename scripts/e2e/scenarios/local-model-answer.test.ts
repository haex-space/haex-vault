import assert from 'node:assert/strict'
import { randomUUID } from 'node:crypto'

import {
  createAndUnlock,
  invokeAfterBusyOperation,
  unwrap,
} from '../lib/flows.ts'
import { pinnedModelFile, QWEN3_0_6B } from '../lib/local-model.ts'
import { onDevice } from '../lib/platform/device-files.ts'
import { scenario } from '../lib/scenario.ts'

// Spec 070 T040 (US5): holzi runs its local models through llama.cpp, on the desktop and on the
// phone. A real model, Qwen3 0.6B (the catalog's smallest, `lib/local-model.ts`), is imported from a
// file, loaded, and answers one message. Run on Linux and on the Android emulator, it shows that the
// engine of each build is llama.cpp and that it answers there.

interface Message {
  role: string
  content: string
  finishReason: string | null
  completionTokens: number | null
}

scenario('local-model-answer', { timeoutMs: 600_000 }, async (ctx) => {
  const model = QWEN3_0_6B
  const hostFile = await pinnedModelFile(model)
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-local-model' })

  // After the start: on Android it clears the app's data, where the staged file lies.
  const installed = unwrap<{ id: string }>(
    'import_model_from_file',
    await invokeAfterBusyOperation(instance, 'import_model_from_file', {
      args: {
        id: model.id,
        name: model.name,
        sourcePath: onDevice(hostFile, instance),
        tokenizerRepo: model.tokenizerRepo,
        // Room for the chat's tools and a short answer, and little memory on the emulator.
        contextWindow: 4096,
        filename: model.filename,
      },
    }),
  )
  assert.equal(installed.id, model.id)
  ctx.step('the model file is imported')

  const loaded = unwrap<{ modelId: string; engine: string | null }>(
    'load_model',
    await invokeAfterBusyOperation(instance, 'load_model', {
      modelId: model.id,
    }),
  )
  assert.equal(loaded.modelId, model.id)
  assert.equal(loaded.engine, 'llama.cpp')
  ctx.step('llama.cpp loads it')

  const sent = unwrap<{ threadId: string }>(
    'send_message',
    await invokeAfterBusyOperation(instance, 'send_message', {
      args: {
        content: 'Antworte mit einem kurzen Satz: Welche Farbe hat der Himmel?',
        maxNewTokens: 96,
        idempotencyKey: randomUUID(),
      },
    }),
  )
  let answer: Message | undefined
  await ctx.waitFor(
    'the answer of the model',
    async () => {
      const messages = unwrap<Message[]>(
        'list_messages',
        await instance.invoke('list_messages', { threadId: sent.threadId }),
      )
      answer = messages.find(
        (message) =>
          message.role === 'assistant' && message.finishReason !== null,
      )
      return answer !== undefined
    },
    { timeoutMs: 300_000, intervalMs: 1000 },
  )
  assert.ok(answer)
  assert.ok(
    answer.finishReason === 'complete',
    `finish reason ${answer.finishReason}: ${answer.content}`,
  )
  assert.ok(answer.content.trim().length > 0, 'an empty answer')
  ctx.step('the model answers')
})
