# Contract: tool-call formats of the llama.cpp engine

Normative for `src-tauri/src/llm/llamacpp/formats/`. A model family is supported when the engine has a
format for it, and a format is chosen from the file's own chat template, never from the model's name. All
three formats were seen in the spike (`research.md`); the examples are real model output.

## What a format provides

| Item             | Meaning                                                                                                          |
| ---------------- | ---------------------------------------------------------------------------------------------------------------- |
| `fingerprint`    | True when the template text carries this format's marks (below). Checked in the order Gemma 4, Qwen XML, Hermes. |
| `shape_messages` | Turns holzi's flat history (`ChatRole::ToolCall` rows, `ToolResult` rows) into what the template reads.          |
| `marker`         | The string a call block starts with; text from its first character on is held back while streaming.              |
| `parse`          | Splits the finished text into `reasoning`, `content` and `calls`.                                                |

A template with tools in the request that matches no fingerprint is **refused**: `StreamError::StartFailed`
with the text "this model's chat template has no tool-call format the engine knows". Without tools in the
request, any template renders.

## Hermes JSON (Qwen3, many others)

Template marks: `<tool_call>` and `"arguments"` in the template, no `<function=`.

```text
<tool_call>
{"name": "open_settings", "arguments": {"page": "sync"}}
</tool_call>
```

- `arguments` is a JSON object, or a string holding one (parsed; a string that is not JSON counts as
  `null` arguments and the call is kept, the tool reports the error).
- Several blocks in one answer are several calls, in order.
- A block that is not valid JSON stays in `content` as plain text and produces no call.

## Qwen XML (Qwen3.5)

Template marks: `<function=` in the template.

```text
<tool_call>
<function=set_label>
<parameter=text>
Kitchen laptop
</parameter>
<parameter=count>
3
</parameter>
</function>
</tool_call>
```

- A value loses exactly one leading and one trailing newline; nothing else is trimmed.
- The value's type comes from the tool's JSON schema: `string` stays a string; any other type is parsed as
  JSON, and a value that does not parse stays a string. A parameter the schema does not name is parsed as JSON
  with the same fallback.
- A block cut off by the token limit (no `</tool_call>`) is parsed up to its end; an unfinished
  `<parameter>` is dropped.

## Gemma 4

Template marks: `<|tool_call>call:` in the template (the check llama.cpp uses too). Settled in T030 (R4)
from the template in `gemma-4-E4B-it-Q4_K_M.gguf` (fixture `gemma4.jinja`) and llama.cpp's reader
(`common/parsers/gemma4.cpp`, `gemma4_to_json`).

```text
<|tool_call>call:open_settings{page:<|"|>sync<|"|>}<tool_call|>
```

- `call:NAME{key:value,…}`. Keys are bare, up to the `:`; a key in string quotes is read as well.
- Strings are wrapped in `<|"|>` and taken as they are: commas, braces, colons and newlines inside stay. A
  string cannot contain `<|"|>` (it is one token). Numbers, `true`, `false` and `null` are bare.
- Objects `{…}` and arrays `[…]` nest; `{}` and `[]` are empty. Spaces between the parts are allowed.
- A bare word that is not a number, boolean or `null` (`{page:sync}`) is kept as a string. The template never
  writes this, but without llama.cpp's grammar the model may.
- `call:NAME{}` and `call:NAME` are calls without arguments. A block without `call:`, with no name, or with
  a name containing spaces stays in `content` as written.
- Several blocks in one answer are several calls, in order. The model stops after its calls: the vocabulary
  marks `<|tool_response>` (and `<turn|>`) as end of generation.
- A block cut off by the token limit (no `<tool_call|>`) is read when its object is complete; an object cut
  off inside stays in `content` as written.

Examples (each is a case in `gemma4_tests.rs`):

| Text inside the block                                       | Call input                                        |
| ----------------------------------------------------------- | ------------------------------------------------- |
| `call:f{a:5,b:-1.5,c:true,d:false,e:null}`                  | `{"a":5,"b":-1.5,"c":true,"d":false,"e":null}`    |
| `call:f{filter:{tags:[<\|"\|>a<\|"\|>],limit:3},ids:[1,2]}` | `{"filter":{"tags":["a"],"limit":3},"ids":[1,2]}` |
| `call:f{text:<\|"\|>a, {b}: c<\|"\|>}`                      | `{"text":"a, {b}: c"}`                            |
| `call:f{ page : <\|"\|>sync<\|"\|> , n : 5 }`               | `{"page":"sync","n":5}`                           |
| `call:f{page:sync}`                                         | `{"page":"sync"}`                                 |
| `call:list_tabs`                                            | `{}`                                              |

**Thinking** sits in `<|channel>thought\n…<channel|>`, not in `<think>`; with thinking on the system turn
starts with `<|think|>`. After a tool result with thinking on, the prompt itself ends in
`<|channel>thought\n`. Gemma 4 may also write an empty thought block, a `<|channel>` that opens no thought,
or a lone `<channel|>` after its answer: all of them are dropped from `content`, and while streaming they
are held back like a call marker.

**History.** Tool results go into the assistant message that made the calls, as
`tool_responses: [{name, response}]`, followed by the answer that came after them as that message's
`content` (llama.cpp's `convert_tool_responses_gemma4` does the same). The current template also reads
OpenAI-style `tool` messages; older Gemma 4 templates read only `tool_responses`, so the merged shape works
with both. A response is matched to its call by the call ID (by position when no ID matches), and a result
that is JSON is passed as JSON, otherwise as text. The template renders it as
`<|tool_response>response:NAME{value:<|"|>…<|"|>}<tool_response|>` and leaves the model turn open, so the
model goes on after the result in the same turn.

## Thinking

The Qwen family's tags are `<think>…</think>`; a format names its own (Gemma 4 above).

- A `</think>` in the output ends a thinking part that the prompt opened (`enable_thinking`); everything
  before it (without a leading `<think>`) is `reasoning`, the rest is `content`.
- With thinking off the template places an empty block in the prompt; the model's output then has no
  `</think>` and is all `content`.

## Finish reasons

`tool_calls` when at least one call was read, `length` when `max_new_tokens` ended the text, `stop`
otherwise.

## Tests

Each format has a fixture directory with the real template of at least one model and a table of
(text, expected parse) cases, including each "open" and "malformed" case named above. A format change is
accepted when its table and the 83-sentence evaluation (spec 032) hold.
