# Requirements Quality Checklist: Structured Agent Tasks for Extensions

**Purpose**: Review whether the structured-agent-task requirements are complete, clear, consistent, and ready for implementation.
**Created**: 2026-10-09
**Feature**: [spec.md](../spec.md)

**Note**: This custom checklist is a reviewer-owned requirements-quality artifact. It evaluates the written requirements, not implementation status.
**Review Ownership**: Mark an item `[x]` only after reviewing the requirement quality.

## Requirement completeness

- [ ] CHK001 Are the host interface, task naming, input schema, output schema, and versioning requirements all defined? [Completeness, Spec FR-001–FR-003]
- [ ] CHK002 Are profile selection, harness selection, processing location, capability requirements, and provenance requirements all specified? [Completeness, Spec FR-004, FR-007]
- [ ] CHK003 Are local processing, remote consent, local-no-fallback, and unavailable-profile behaviors covered together? [Completeness, Spec FR-006, FR-012]
- [ ] CHK004 Are cancellation, timeout, invalid-result, provider-failure, and original-data-preservation requirements defined? [Completeness, Spec FR-008, FR-010]
- [ ] CHK005 Does the scope clearly distinguish the first structured task from the later personal-handwriting/RAG functionality? [Scope, Spec Out of Scope, Assumptions]

## Requirement clarity

- [ ] CHK006 Is “structured result” defined precisely enough to distinguish valid task output from arbitrary model text? [Clarity, Spec FR-003]
- [ ] CHK007 Are the accepted image representations, MIME-type expectations, dimensions, size limits, and local-reference rules explicit? [Clarity, Spec FR-002]
- [ ] CHK008 Are the required capabilities for image input, OCR, structured output, and layout regions distinguishable from general chat capabilities? [Clarity, Spec FR-004, Key Entities]
- [ ] CHK009 Is “visible consent” defined in terms of the information shown to the user, including processing mode and remote destination? [Clarity, Spec FR-006, SC-004]
- [ ] CHK010 Are profile identifiers, model identifiers, harness identifiers, and remote destinations defined as non-secret provenance values? [Clarity, Spec FR-007, FR-011]

## Consistency and boundaries

- [ ] CHK011 Are the asynchronous start/completion/cancellation semantics consistent between the task contract, data model, and implementation task ordering? [Consistency, Plan, contracts/task.md, Tasks T013–T018]
- [ ] CHK012 Does the consent model consistently prevent an extension from approving its own remote transfer? [Consistency, Spec FR-006, contracts/task.md]
- [ ] CHK013 Are “extension,” “frame,” “profile,” “harness,” “provider,” and “model” used consistently across all artifacts? [Consistency, Key Entities, Plan, contracts/task.md]
- [ ] CHK014 Is the boundary between structured tasks and the free-form agent/chat API explicit and free of conflicting requirements? [Boundary, Spec Out of Scope, Plan Structure Decision]

## Acceptance criteria quality

- [ ] CHK015 Can schema validity, unsupported-capability rejection, and provenance correctness be objectively assessed from the stated success criteria? [Measurability, Spec SC-001, SC-002, SC-006]
- [ ] CHK016 Can local locality and remote-consent guarantees be objectively assessed without relying on an unobservable implementation assumption? [Measurability, Spec SC-003, SC-004]
- [ ] CHK017 Does the requirement set define what counts as preserving the original input after cancellation or failure? [Measurability, Spec SC-005]
- [ ] CHK018 Are the performance limits for task start, bridge non-blocking behavior, and completion timeout explicit and aligned between spec and plan? [Measurability, Plan Performance Goals]

## Scenario and edge-case coverage

- [ ] CHK019 Are primary, alternate, exception, and recovery requirements present for valid input, unavailable local model, remote denial, invalid model output, timeout, cancellation, and frame closure? [Scenario Coverage, Spec User Stories, Edge Cases]
- [ ] CHK020 Are concurrent-task limits and behavior at the per-extension/per-frame boundary specified rather than left to an implementation default? [Gap, Plan Scale/Scope, Tasks T005]
- [ ] CHK021 Does the specification define the user-facing behavior when a local Vision/OCR model is not installed and no remote fallback is allowed? [Scenario Coverage, Spec Edge Cases, FR-012]
- [ ] CHK022 Are extension disable/remove and vault-close recovery semantics specified with the same precision as frame-close cancellation? [Gap, Spec FR-010, contracts/task.md]

## Security, privacy, and dependencies

- [ ] CHK023 Are credentials, raw provider responses, arbitrary file paths, general tools, and shell access explicitly excluded from every task boundary? [Security, Spec FR-009, FR-011, contracts/task.md]
- [ ] CHK024 Are remote destinations, consent expiry, denial, and cancellation requirements sufficient to prevent accidental image transfer? [Privacy, Spec FR-006, Edge Cases]
- [ ] CHK025 Are the existing HaexVault provider/model preferences and the required local Vision/OCR backend documented as validated dependencies rather than assumed capabilities? [Dependency, Spec Assumptions, Plan Technical Context]
- [ ] CHK026 Does the contract identify which values are persisted, which are in-memory only, and which may cross the extension boundary? [Dependency, data-model.md, contracts/task.md]

## Notes

- Leave every item unchecked until the reviewer confirms the written requirements are ready.
- `[Gap]` marks a deliberate review question where the current artifacts may need refinement before implementation.
- This checklist does not certify code or test behavior.
