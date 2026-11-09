# Feature Specification: Project-scoped build environment

**Feature Branch**: `077-project-build-environment`

**Created**: 2026-11-02

**Status**: Draft

**Input**: Remove the global Android and CUDA build exports from the personal shell configuration and make the project's development environment sufficient to start the CUDA desktop application, including discovery of the installed host WebKit libraries.

## User Scenarios & Testing

### User Story 1 - Start the CUDA desktop app from the project environment (Priority: P1)

A developer enters the project's development environment from a shell without personal build exports and starts the CUDA desktop application using its existing command.

**Why this priority**: Global Android compiler settings currently break desktop compilation, and the project's environment fails to find installed host WebKit libraries on a multiarch Linux installation.

**Independent Test**: Start the existing CUDA development command from a shell without the personal Android or CUDA exports and verify that compilation succeeds and the application process starts.

**Acceptance Scenarios**:

1. **Given** the documented development prerequisites and an uncustomized build environment, **When** the developer enters the project environment and starts the CUDA desktop application, **Then** compilation finds the host C++ headers, CUDA libraries, and installed WebKit libraries without personal shell exports.
2. **Given** a host that installs library metadata in a multiarch directory, **When** the project checks its WebKit prerequisites, **Then** both WebKit and JavaScriptCore are found.
3. **Given** a host that uses the existing non-multiarch layout, **When** the project checks its prerequisites, **Then** discovery continues to work.

### User Story 2 - Keep build settings inside the project (Priority: P2)

A developer removes the personal Android and CUDA build configuration without losing the project's supplied development tools or imposing those settings on unrelated projects.

**Why this priority**: The build must not depend on hidden personal configuration or leak an Android compiler into desktop work.

**Independent Test**: Inspect a fresh ordinary shell and the project environment separately: the ordinary shell has no project-specific build exports, while the project environment supplies its documented Android and CUDA prerequisites.

**Acceptance Scenarios**:

1. **Given** the successful project-environment startup check, **When** the obsolete personal build exports and temporary CUDA layout are removed, **Then** the same startup command still works inside the project environment.
2. **Given** an ordinary shell outside the project, **When** it is started after cleanup, **Then** it does not select an Android compiler or a Holzi-specific CUDA path globally.
3. **Given** the existing Android workflow, **When** its project environment is entered, **Then** its SDK, NDK, and Java remain available without the removed personal exports.

### Edge Cases

- A required host library is genuinely absent; discovery must not claim success.
- The host provides additional library metadata search paths or no separate host discovery executable.
- An existing terminal still retains exports inherited before the cleanup.
- The command runs outside Linux or outside the project development environment; existing supported behavior is preserved.
- A frontend runtime error occurs after successful startup; the result distinguishes that error from build-environment failures.

## Requirements

### Functional Requirements

- **FR-001**: The documented project environment MUST supply the build settings required for the CUDA desktop startup without personal shell build exports.
- **FR-002**: Host library discovery MUST support both the existing Linux layout and the host's multiarch layout without embedding a developer-specific path.
- **FR-003**: Desktop compilation MUST use a compiler for the desktop target; Android compiler selection MUST remain confined to Android builds.
- **FR-004**: The existing project-provided Android SDK, NDK, and Java configuration MUST remain available after personal shell cleanup.
- **FR-005**: The temporary personal CUDA workaround and obsolete global build exports MUST be removed only after their project-scoped replacement is verified; unrelated personal shell configuration MUST be preserved.
- **FR-006**: The startup instructions MUST explain how to enter the project environment and clear stale exports from an already-open terminal.
- **FR-007**: Validation MUST include executable coverage of host library discovery and an actual CUDA development startup, reporting any remaining unrelated failure separately.

## Success Criteria

### Measurable Outcomes

- **SC-001**: The existing CUDA development command completes compilation and starts the application process from the project environment with zero personal build exports required.
- **SC-002**: Host-library discovery checks pass for both supported directory layouts and reject genuinely missing libraries.
- **SC-003**: A fresh ordinary shell contains none of the removed Android compiler or Holzi CUDA exports, while the project environment still exposes its documented Android and CUDA prerequisites.

## Assumptions

- The developer uses the existing project development environment for Holzi builds.
- Host graphics drivers and the documented host WebKit libraries are installed.
- This work repairs the build environment; the separately observed Vue rendering error is outside its scope.
- The personal shell cleanup applies only to the build settings identified in this session and does not change unrelated tools or credentials.
