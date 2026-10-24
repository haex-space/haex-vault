// The test extension of spec 055 (T065): sends the page recognition of `contracts/job-format.md`
// §Beispiel to holzi's models through the released vault-sdk. `build.sh` bundles this file with the
// SDK into app.js.
//
// `access` and `settings` are the way to the grant (US9, T092): ask before the first job, open holzi's page of this extension's models.
// The test modes ask the model for broken output (a loop, nothing, Chinese) that holzi must drop. A
// real model mostly obeys; the stand-in server of the e2e scene answers by the `[holzi-test:…]` mark.
import { createHaexVaultSdk } from '@haex-space/vault-sdk'
import manifest from './manifest.json'
import pageFormat from './page-format.json'

const sdk = createHaexVaultSdk({ manifest })

// One pixel, read when no photo is chosen.
const PIXEL = {
  mimeType: 'image/png',
  data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
}

const READ = `This is a photo of a student's handwritten notes, a blackboard picture or a worksheet, in German.
Split the page into regions and return ONLY a JSON object:
{"regions": [{"kind": "text" | "table" | "image", "box": [x1, y1, x2, y2], "text": "...", "rows": [["cell", ...], ...]}], "rotation": 0}
Rules:
- box uses coordinates 0-1000 relative to the image width and height.
- "text": transcribe the handwriting exactly as written, keep line breaks, do not correct anything.
- "table": give "rows" as a list of rows of cell strings, an empty cell as "", no "text".
- "image": drawings, diagrams, sketches and photos, no "text".
- List regions in reading order. Do not invent content that is not on the page.
- "rotation": the clockwise degrees (0, 90, 180 or 270) the photo must be turned to stand upright.`

const FIX = `These are the regions of a German page read from handwriting. Correct only spelling and words
that were clearly misread in their "text"; keep abbreviations, line breaks and the wording otherwise.
Return ONLY {"texts": [...]} with one corrected text per region that has a "text", in the same order.`

const TEST_MODES = {
  loop: '[holzi-test:loop] Answer only with the word "ja", repeated 40 times.',
  empty: '[holzi-test:empty] Answer with nothing at all.',
  cjk: '[holzi-test:cjk] Answer only in Chinese characters.',
}

const textFix = {
  name: 'text-fix',
  version: 1,
  schema: {
    type: 'object',
    required: ['texts'],
    properties: { texts: { type: 'array', items: { type: 'string' } } },
  },
}

function read(image, extra) {
  return {
    id: 'read',
    needs: ['text', 'image'],
    instruction: extra ? `${READ}\n${extra}` : READ,
    input: { images: [image] },
    format: pageFormat,
    effort: 'medium',
    maxOutputTokens: 8192,
  }
}

/** The job of `mode`: the page read and corrected, or one read step asking for broken output. */
function jobOf(mode, image) {
  if (mode !== 'page') return { steps: [read(image, TEST_MODES[mode])] }
  return {
    steps: [
      read(image),
      {
        id: 'fix',
        needs: ['text'],
        instruction: FIX,
        input: { from: { step: 'read', pointer: '/regions' } },
        format: textFix,
        effort: 'low',
      },
    ],
  }
}

async function photo() {
  const file = document.getElementById('photo').files?.[0]
  if (!file) return PIXEL
  const bytes = new Uint8Array(await file.arrayBuffer())
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return { mimeType: file.type, data: btoa(binary) }
}

function show(status, value) {
  document.getElementById('status').textContent = status
  document.getElementById('result').textContent = JSON.stringify(value, null, 2)
}

// The e2e scene starts a job and reads `last` once it is set; the SDK answers holzi's questions by
// waiting for the user, so `run` returns before the job ends.
window.modelJobs = {
  last: undefined,
  run(mode) {
    this.last = undefined
    show('läuft', null)
    if (mode === 'access' || mode === 'settings') return this.lead(mode)
    photo()
      .then((image) => sdk.models.runAsync(jobOf(mode, image)))
      .then((end) => {
        this.last = end
        show(end.status, end)
      })
      .catch((error) => {
        this.last = {
          rejected: {
            code: error?.code,
            message: error?.message,
            details: error?.details,
          },
        }
        show('abgelehnt', this.last)
      })
  },
  /** The way to the grant (spec 055 US9): ask before the first job, or open holzi's model page. */
  lead(mode) {
    const end =
      mode === 'access'
        ? sdk.models.requestAccessAsync({
            needs: ['text', 'image'],
            effort: 'low',
          })
        : sdk.models.openSettingsAsync().then(() => ({ opened: true }))
    end
      .then((answer) => {
        this.last = mode === 'access' ? { models: answer } : answer
        show(mode, this.last)
      })
      .catch((error) => {
        this.last = {
          rejected: {
            code: error?.code,
            message: error?.message,
            details: error?.details,
          },
        }
        show('abgelehnt', this.last)
      })
  },
}

for (const button of document.querySelectorAll('button[data-mode]')) {
  button.addEventListener('click', () =>
    window.modelJobs.run(button.dataset.mode),
  )
}
