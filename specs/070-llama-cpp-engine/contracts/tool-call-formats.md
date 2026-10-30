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

## Gemma 4 (provisional, completed in T030)

Template marks: `<|tool_call>` in the template.

```text
<|tool_call>call:open_settings{page:<|"|>sync<|"|>}<tool_call|>
```

- `call:NAME{key:value,…}`; strings are wrapped in `<|"|>`; numbers, booleans and `null` are bare.
- Open: nested objects, arrays, strings that contain `,` `}` or `<|"|>`, several calls in one turn, tool
  results (the template merges them into the preceding assistant turn). T030 fills these in from the
  template in the model file and the llama.cpp sources, with a fixture per case, before `gemma4.rs` is written.

## Thinking

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
