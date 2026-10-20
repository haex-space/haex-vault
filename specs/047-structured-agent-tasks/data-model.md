# Data Model: Structured Agent Tasks

> **Abgelöst:** Dieses Datenmodell ist ein historischer Entwurf aus Spec 047.
> Für neue Arbeit gilt ausschließlich das Datenmodell in
> [Spec 055](../055-extension-model-access/contracts/job-format.md).

## `AgentTaskDefinition`

| Field                  | Type             | Rules                                                                      |
| ---------------------- | ---------------- | -------------------------------------------------------------------------- |
| `task`                 | string           | Stable identifier; first value is `paper-note-recognition`.                |
| `schemaVersion`        | positive integer | Host and extension must agree before execution.                            |
| `requiredCapabilities` | set              | Includes image input, OCR/vision and structured output for the first task. |
| `maxInputBytes`        | positive integer | Reject oversized images before a task is created.                          |
| `timeoutSeconds`       | positive integer | Bounded by the host maximum of 180 seconds.                                |

## `AgentTaskProfile`

| Field         | Type              | Rules                                                                       |
| ------------- | ----------------- | --------------------------------------------------------------------------- |
| `profileId`   | string            | User-visible stable identifier; never a secret.                             |
| `task`        | string            | The task this profile is allowed to run.                                    |
| `providerId`  | UUID/string       | Refers to an existing host provider or the local provider.                  |
| `modelId`     | string            | Refers to an installed or cached model row.                                 |
| `harnessId`   | string            | Identifies the execution harness, not credentials.                          |
| `mode`        | `local \| remote` | Derived from the resolved provider; not blindly trusted from the extension. |
| `destination` | string?           | User-readable remote destination; omitted for local mode.                   |

Profiles are persisted as preferences, not as extension-owned database rows.
Credentials remain in the host's provider storage.

## `AgentTaskRun`

| Field         | Type                                                              | Rules                                                                 |
| ------------- | ----------------------------------------------------------------- | --------------------------------------------------------------------- |
| `taskId`      | opaque string                                                     | Unique per run; never accepted from another extension.                |
| `extensionId` | UUID                                                              | Captured from the authenticated frame session.                        |
| `frame`       | string                                                            | The originating frame; completion events are scoped to its extension. |
| `task`        | string                                                            | Must match a registered task definition.                              |
| `status`      | `consent_required \| running \| completed \| failed \| cancelled` | `consent_required` is pending, terminal states are immutable.         |
| `processing`  | provenance                                                        | Contains actual profile/model/harness/mode, without secrets.          |
| `cancel`      | cancellation token                                                | In-memory only; frame close and vault close cancel it.                |
| `result`      | JSON?                                                             | Present only after schema validation.                                 |
| `error`       | structured error?                                                 | User-readable marker without raw provider data or paths.              |

## State transitions

```text
start request
  ├─ unsupported/profile unavailable → no run
  ├─ consent required → consent_required → running
  └─ accepted → running → completed
                              ├─ failed
                              └─ cancelled
```

## Validation invariants

- A task run cannot outlive its originating frame or active vault.
- A local profile cannot emit a remote destination or invoke a remote fallback.
- A result is emitted only after the task-specific schema validator succeeds.
- Image bytes and raw model output are not persisted by Holzi after completion.
- Extension-visible errors never include provider credentials, full local paths,
  or unbounded raw model output.
