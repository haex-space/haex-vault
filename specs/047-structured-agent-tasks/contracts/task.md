# Structured Agent Task Contract

> **Abgelöst:** Dieser Vertrag ist ein historischer Entwurf aus Spec 047. Die
> aktuelle Brücke und das aktuelle Auftragsformat stehen in
> [Spec 055](../../055-extension-model-access/contracts/bridge.md). Die unten
> genannten `extension_ai_task_*`-Methoden dürfen nicht implementiert oder
> aufgerufen werden.

This contract describes the host boundary used by extensions such as
haex-notes. It is model- and harness-neutral and intentionally asynchronous.

## Start request

Bridge method: `extension_ai_task_start`

```text
task: "paper-note-recognition"
schemaVersion: 1
profileId?: string
input:
  image:
    bytes: binary
    mimeType: string
    width: positive integer
    height: positive integer
  locale: string
  options:
    detectDrawings: boolean
    suggestSpelling: boolean
```

The host resolves `profileId` to the user's configured profile. If it is
omitted, the profile configured as the default for this task is used. The
request never contains provider credentials.

The immediate response is:

```text
taskId?: string  # absent when the start is refused (unsupported_capability, profile_unavailable): no run exists
status: "running" | "consent_required" | "unsupported_capability"
processing?:
  mode: "local" | "remote"
  profileId: string
  modelId?: string
  harnessId?: string
  destination?: string
consentRequestId?: string
error?:
  code: "consent_required" | "unsupported_capability" | "profile_unavailable"
  message: string
```

When `status` is `consent_required`, no image bytes are sent to a provider.
Holzi creates an opaque, short-lived `consentRequestId` and shows the
processing mode and destination in its own user interface. The extension
cannot approve its own request. Holzi's own window resolves the request using
the host-only command:

```text
agent_task_consent_resolve

taskId: string
consentRequestId: string
approved: boolean
```

An approval resumes the pending task with the already selected profile. A
denial emits the normal completion event with
`error.code: "consent_denied"`. Closing the consent UI, cancelling the task,
or expiring the request also prevents any provider request.

## Completion event

Event: `haextension:ai-task:completed`

The event is delivered only to frames of the extension that started the task.

```text
taskId: string
status: "completed" | "failed" | "cancelled"
processing:
  mode: "local" | "remote"
  profileId: string
  modelId: string
  harnessId: string
  destination?: string
result?:
  schemaVersion: 1
  regions: [...]  # task-specific, validated by the host
error?:
  code: "invalid_result" | "timeout" | "cancelled" | "consent_denied" | "inference_failed"
  message: string
```

The `regions` payload for `paper-note-recognition` contains normalized
text/drawing/unknown regions, optional text, confidence values,
spelling/recognition suggestions and source boxes. The host validates that
payload before emitting the completion event.

## Cancellation

Bridge method: `extension_ai_task_cancel`

```text
taskId: string
```

Cancellation is idempotent. Closing the originating frame cancels all tasks
owned by that frame. A task may not continue after the originating extension
has been disabled or removed.

## Security rules

- A task is addressed to a specific active extension and cannot be invoked by a
  different extension.
- The task receives only the declared input and no general agent tools by
  default.
- Remote processing is opt-in and reports its destination before bytes leave
  the device.
- Local processing does not use a remote fallback implicitly.
- Messages must not contain credentials, full local paths, or raw provider
  responses.
- The original input remains the extension's responsibility and is never
  mutated by the host.
