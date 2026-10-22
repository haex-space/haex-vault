# Feature Specification: End-to-End Tests Across Several Vaults and Devices

**Feature Branch**: `033-multi-device-e2e`

**Created**: 2026-10-01

**Status**: Draft

**Input**: User description: "The nine manual checks M1 to M9 of spec 024 (own-device sync) run as
automatic end-to-end scenarios on the rig of spec 016: several real application processes with their
own data, a test Nostr relay, driven through the real interface. For that the rig gets general
building blocks for several vaults and devices (named groups, per-device network control, control of
the waiting time for the 60-second online state, reading online state and device list, copying a
vault file, a scenario template for several vaults with different users) that later features (own
file sync, spaces, data shares, haextensions) only plug into. Linux only for now, but scenarios must
not depend on the Linux driver, so that Windows, macOS, Android and iOS can be added later, each by
its own spec."

## Clarifications

### Session 2026-10-01

- Q: How wide is this spec on platforms? → A: Linux only. The scenarios are written against a driver layer that
  hides the platform, and the spec names what each further platform (Windows, macOS, Android, iOS) would need,
  but implements none of them; each gets its own follow-up spec.
- Q: Does the spec fix the building blocks for later features too? → A: Yes, as general multi-vault helpers
  (named groups, per-device network control, fixed real-time deadlines, online state and device list, vault-file
  copy, scenario template). Later specs (025, 028, 056, haextensions) add scenarios on top and do not
  rebuild the helpers.

## User Scenarios & Testing _(mandatory)_

The users are the maintainers and contributors who write and run these tests, and the later specs whose
features need to be tested across several vaults. Each story can be delivered and used on its own.

### User Story 1 - Linking and syncing, checked through the real interface (Priority: P1)

A maintainer runs one scenario and gets the verdict that two real application processes can be linked and
then keep each other's data current, observed the way a person sees it: on the start page, in the device
view and in the chat. This replaces the manual checks M1 (linking), M2 (sync) and M3 (roles and identity).

**Why this priority**: Linking and sync are the base of every later multi-vault feature, and the
existing scenarios only drive them through backend commands, so the interface people use is untested.

**Independent Test**: Run the scenario. Device A creates a vault and shows a link code. Device B uses the
start-page form "Mit einer Vault verknüpfen", enters the code, a device name and a passphrase. A shows B's
name and the role question, which the scenario leaves at "no", and confirms. B opens the vault with all
of A's data. Both show B as "verknüpftes Gerät" and online, and show the same public vault identity.

**Acceptance Scenarios**:

1. **Given** device A with a vault and data, **When** B links through the start-page form and A confirms with
   the role left at "no", **Then** B opens the vault with all data, and both devices list B as linked
   device, online.
2. **Given** linked devices, **When** a chat is started on A, **Then** it appears on B within the sync time
   promised by spec 024; **When** a setting is changed on B, **Then** it applies on A.
3. **Given** linked devices, **When** B is stopped, A keeps working, and B is started again, **Then** B
   catches up on everything done on A in the meantime.
4. **Given** linked devices, **When** the device view is opened on both, **Then** both show the same public
   vault identity, it can be copied and cannot be edited; on B the actions "Gerät verknüpfen" and
   "Gerät entfernen" are absent.
5. **Given** a join with a refused role question or a refused confirmation, **When** A declines, **Then** B
   ends without the vault.

---

### User Story 2 - Online state and the servers that devices find each other through (Priority: P1)

A maintainer can check that the device view tells the truth about which devices are reachable, and that
switching off the connection servers stops devices finding each other without stopping local work. This
replaces M4 (online state) and M9 (servers).

**Why this priority**: These two promises (spec 024 FR-008 and the online state of user story 4) are
what users rely on when something looks wrong, and they need the network to be controllable per device,
which every later multi-device scenario needs as well.

**Independent Test**: Link A and B. Stop B (or make it unreachable while the app runs): within 60 seconds A
shows B as "zuletzt online" with the matching time. Switch off all Nostr servers on both: they no longer
find each other, local work on each goes on, and after switching them on again they find each other again
and exchange what was done in between.

**Acceptance Scenarios**:

1. **Given** linked devices, **When** B stops or becomes unreachable, **Then** A shows B as "zuletzt online"
   with a time that matches, within 60 seconds.
2. **Given** linked devices, **When** all Nostr servers are switched off, **Then** the devices no longer
   find each other, work done locally continues to succeed, and nothing about this blocks the interface.
3. **Given** the situation of scenario 2, **When** the servers are switched on again, **Then** the devices
   find each other again, and the changes made in the meantime reach the other device.
4. **Given** a scenario that needs a wait of 60 seconds, **When** it runs in the ordinary suite, **Then**
   it does not make the run wait that long in real time, unless no other way exists (see Assumptions).

---

### User Story 3 - Removing devices, including both main devices removing each other (Priority: P2)

A maintainer can check that removing a device does what the consequences dialog promises, and that two
main devices that remove each other while offline end in the same state everywhere. This replaces M6
(removal) and M7 (mutual removal).

**Why this priority**: Removal and its conflict case protect users after a lost device. They are risky and
easy to break, but only needed after linking and presence work.

**Independent Test**: With main devices A and C and linked device B and copy D: remove D on A. The dialog
names the consequences before confirming. D receives nothing new and shows that it was removed; A, B and C
keep syncing. Then take A and C offline, remove C on A and A on C, bring both online again: on all devices
the same one of the two main devices remains (the one whose list has the smaller hash), the other is removed
and says so, and B still has a main device.

**Acceptance Scenarios**:

1. **Given** devices A, B, C and D, **When** A starts removing D, **Then** the dialog states the
   consequences before the confirmation, and confirming removes D.
2. **Given** D was removed, **When** work continues on A, B and C, **Then** D receives nothing new and shows
   that it was removed, and A, B and C go on syncing.
3. **Given** main devices A and C both offline, **When** each removes the other and both come online, **Then**
   all devices agree on the same remaining main device, the other shows it was removed, and the scenario
   can tell which of the two must remain.

---

### User Story 4 - Copies of the vault file (Priority: P2)

A maintainer can check that a copy of a main device's vault file becomes a working main device, and that a
copy of a linked device's file waits for admission and then catches up. This completes M5.

**Why this priority**: Copying a file is a way users will actually add a device. The existing scenario shows
only the notice and injects state; it never syncs.

**Independent Test**: Copy the file of A (main) to a new location and open it as device C: C shows the
notice, is a main device and syncs with A. Copy the file of B (linked) to device D and open it: D shows
"wartet auf Aufnahme"; D makes a change while waiting; on A the admission request shows and "Aufnehmen" is
chosen; D syncs, and its change from the waiting time arrives everywhere.

**Acceptance Scenarios**:

1. **Given** a copy of a main device's file, **When** it is opened as C, **Then** the notice shows, C is
   a main device and syncs with A in both directions.
2. **Given** a copy of a linked device's file, **When** it is opened as D, **Then** D shows that it waits
   for admission and neither sends nor receives changes.
3. **Given** D made a change while waiting, **When** A admits D through the real "Aufnehmen" action,
   **Then** D syncs and its change from the waiting time reaches the other devices.
4. **Given** the request on A, **When** A chooses "Ablehnen", **Then** D stays outside.

---

### User Story 5 - Locking during a large sync (Priority: P2)

A maintainer can check that locking a vault while a large sync is running ends all connections at once
and that the next opening completes the sync with nothing missing. This replaces M8.

**Why this priority**: It combines the vault-close promises of spec 013 with the sync of spec 024; a leak
here keeps connections open to a vault that looks closed.

**Independent Test**: Give A enough data that a sync to B takes noticeable time. Start it, lock A's vault
during it. All connections end immediately. Unlock: the sync continues and B ends with all data, none
missing and none duplicated.

**Acceptance Scenarios**:

1. **Given** a running sync of a large amount of data, **When** the vault on A is locked, **Then** every
   connection of A to other devices ends at once and the other devices see A as gone.
2. **Given** the vault was locked mid-sync, **When** it is opened again, **Then** the sync continues and
   ends with all data on B, with nothing missing and nothing duplicated.

---

### User Story 6 - Writing a scenario with several vaults and users takes few lines (Priority: P2)

A contributor who adds a feature that spans vaults (own file sync, spaces, data shares, a haextension)
writes a scenario by naming the vaults, the users and the devices, saying how they are connected, and
then driving and checking them. The helpers for groups, network, fixed real-time deadlines, device state and copying a vault
file already exist.

**Why this priority**: Spec 025, 028 and 056 and the haextensions all need exactly these abilities, and
building them again per feature would be wasteful and would diverge.

**Independent Test**: A reviewer who has not seen the helpers writes a scenario with two users, each with
two devices, where one device of the first user is made unreachable and brought back, using only the
documentation and the template, in 60 lines or fewer.

**Acceptance Scenarios**:

1. **Given** the scenario template, **When** a contributor names two users with two devices each, **Then**
   the four application processes start with separate data, and each can be addressed by its name.
2. **Given** a running group, **When** a scenario makes one device unreachable, **Then** that device is
   running and works locally but neither reaches nor is reached by the others and the test relay, until
   the scenario restores it.
3. **Given** a scenario, **When** it needs the device list, a device's online state or a copy of a vault
   file, **Then** a helper provides it without the scenario touching the driver, the process or the file
   layout directly.
4. **Given** several vaults of different users sharing one test relay, **When** the scenario runs, **Then**
   devices of one user's vault never see another user's data unless the feature under test shares it.

---

### User Story 7 - The platform stays out of the scenarios (Priority: P3)

A maintainer who later wants the same scenarios on Windows, macOS, Android or iOS adds a driver for that
platform without rewriting scenarios. The spec states what each platform would need, so that those
follow-up specs start from facts.

**Why this priority**: The product will ship on five platforms, but each needs a different driver, device
or runner; doing them now would make this spec large and hard to accept. Keeping the seam clean now is
cheap, and cleaning it later is not.

**Independent Test**: Read all scenarios of this spec and find no use of the Linux driver, the virtual
screen, process signals or file paths of the host. Read the platform document and find, for each of the
four further platforms, what is needed to run a scenario there.

**Acceptance Scenarios**:

1. **Given** the scenarios of this spec, **When** they are searched for platform specifics, **Then** none
   is found; they use only operations of the driver layer (start and stop a device, click, type, read,
   call a backend command, wait for something, take a screenshot, and control a running device's network).
2. **Given** the platform document, **When** a maintainer reads the entry of a platform, **Then** it names
   the driver, the runner or device type, how several devices would be connected there and the known
   limits, without implementing any of it.

---

### User Story 8 - The new scenarios run in the existing CI job (Priority: P3)

The scenarios run where the maintainers' other end-to-end scenarios already run, and a failure leaves the
same diagnostic material.

**Why this priority**: A check nobody runs protects nothing, but the job and its stock Linux runner exist.

**Independent Test**: Push a branch with a deliberately broken sync: the end-to-end job fails, and its
result names the scenario and keeps the material of the failure.

**Acceptance Scenarios**:

1. **Given** the existing end-to-end job, **When** it runs, **Then** it runs the new scenarios in addition
   to the old ones within its time limit.
2. **Given** a failing scenario, **When** the job ends, **Then** it names the scenario and keeps the
   failure material for every device that took part.

---

### Edge Cases

- A device fails to start or hangs during a scenario with several devices: the run stops that scenario
  within its time limit, reports which device it was, and leaves no process behind.
- The test relay is switched off or restarted while devices are running: a scenario can do so, and the
  devices neither crash nor lose local work.
- A device that was made unreachable is stopped before it is restored: the scenario ends cleanly and the
  network setting does not leak into the next scenario.
- Two main devices remove each other and their lists have equal hashes: the scenario does not depend on a
  tie; it states what must hold for any outcome (one main device remains, all agree).
- A scenario with more devices than the machine can run at the same time: it fails early with a message
  naming the limit instead of hanging or crashing the machine.
- A copied vault file is taken while its source is still writing: the copy helper copies a consistent state
  of the whole vault file or reports that it could not.
- Time-dependent checks are run on a loaded machine: they use bounds that tolerate slowness and report a
  non-conformant run instead of a false failure, as spec 016 does for scaled time.
- Two scenarios use the same user name: users of different scenarios never share data, ports or relays.

## Requirements _(mandatory)_

### Functional Requirements

**Scenarios replacing the manual checks**

- **FR-001**: The suite MUST contain one automatic scenario for each of the nine manual checks M1 to M9 of
  spec 024, and each scenario MUST check every expectation written for its manual check in
  `specs/024-own-device-sync/quickstart.md`, observed through the real interface wherever the manual
  check names one.
- **FR-002**: The linking scenario MUST drive device B through the start-page form, enter the code, device
  name and passphrase there, and answer the role question on A with "no". It MUST check that B opens the
  vault with all data and that both devices list B as linked device and online (M1).
- **FR-003**: The sync scenario MUST create a chat through the chat interface on A, check that it appears
  on B, change a setting on B and check that it applies on A, and stop B, work on A, start B and check
  that B catches up (M2).
- **FR-004**: The identity scenario MUST check that both devices show the same public vault identity, that
  it can be copied and not edited, and that on a linked device "Gerät verknüpfen" and "Gerät entfernen"
  are absent (M3).
- **FR-005**: The presence scenario MUST check that after a device stops or becomes unreachable, the other
  shows it as "zuletzt online" with a matching time within 60 seconds (M4).
- **FR-006**: The copy scenarios MUST check that a copy of a main device's file becomes a main device with
  the notice and syncs, and that a copy of a linked device's file waits for admission, is admitted through
  the real action, and then syncs, including its changes from the waiting time. They MUST also check the
  refusal path (M5).
- **FR-007**: The removal scenario MUST check that the consequences are shown before confirming, that the
  removed device receives nothing new and shows that it was removed, and that the others go on syncing
  (M6).
- **FR-008**: The mutual-removal scenario MUST take two main devices offline, let each remove the other,
  bring both online, and check that all devices agree on the same remaining main device, that the other
  shows it was removed, and that the linked device still has a main device (M7). Which of the two remains
  is decided by the smaller list hash, which is covered by the unit tests of the device list; the scenario
  checks what must hold for either outcome.
- **FR-009**: The lock scenario MUST lock a vault during a large sync, check that all connections end at
  once, and check that the next opening completes the sync with nothing missing and nothing duplicated
  (M8).
- **FR-010**: The servers scenario MUST switch off all Nostr servers, check that the devices no longer find
  each other and that local work goes on, switch them on again and check that they find each other and
  exchange what happened in between (M9).
- **FR-011**: Once the nine scenarios meet SC-002 in CI, the manual section of the quickstart of spec 024
  MUST be replaced by a pointer to the scenarios, and task T081 of spec 024 MUST be closed with the
  scenario run as its record.

**Building blocks for several vaults and devices**

- **FR-012**: The rig MUST let a scenario declare a group of users and devices with names, start them with
  separate data, and address each by name. Each user has exactly one vault in the group. Several users with
  their own vaults in one scenario MUST be possible, each with several devices.
- **FR-013**: The rig MUST let a scenario control the network per device: make a device unreachable for
  the other devices and the test relay and restore it, while the device stays up and usable for local
  work in between (applying either change may restart the application once; plan, research R2), and
  switch the test relay itself off and on.
- **FR-014**: Time-based expectations MUST use a fixed deadline equal to the promise they check, which the
  time scale of the suite does not stretch. The 60-second expectation of M4 waits in real time and ends as
  soon as the expectation holds, within the bounds of SC-004. The application is not changed to let a
  scenario move its clock (plan, research R1).
- **FR-015**: The rig MUST give a scenario read access to a device's device list, its own role and the
  online state of other devices, as the interface shows them.
- **FR-016**: The rig MUST provide a helper that copies a vault file consistently from one named device to
  another device's data and reports failure instead of a partial copy. The helper MUST use the driver
  layer's vault-file-copy operation and MUST NOT expose host paths to scenarios or helpers.
- **FR-017**: The rig MUST provide a scenario template for several vaults with different users, and
  documentation with an example, so that a contributor needs no knowledge of how processes, ports,
  screens or relays are arranged.
- **FR-018**: Every device of a scenario MUST have its own data, ports and virtual screen, as in spec 016,
  and the suite MUST end all of them, the test relay and anything else it started when a scenario ends,
  fails, times out or is interrupted.
- **FR-019**: A scenario that needs more devices than the machine can run MUST fail at its start with a
  message that names the limit; the suite MUST NOT start devices it cannot keep running.
- **FR-020**: A failing scenario MUST keep the failure material of spec 016 (screenshots, logs, the state
  of the data) for every device that took part, labelled by device name.

**Platform seam**

- **FR-021**: Scenarios and helpers MUST use only operations of a driver layer that hides the platform
  (start and stop a device, click, type, read, call a backend command, wait for a condition, take a
  screenshot, control a running device's network, and copy a vault file between named devices). They
  MUST NOT use the Linux driver, the virtual screen, process signals or host file paths directly; those
  belong to the one Linux implementation of the layer. The vault-file-copy operation MUST accept device
  identities rather than host paths and MUST report a failed copy without leaving a partial destination.
- **FR-022**: The spec MUST be accompanied by a platform document that names, for Windows, macOS, Android
  and iOS, the driver, the runner or device type, how several devices would be connected there, and the
  known limits. It MUST NOT implement any of them. (Since spec 043 the Android part is implemented;
  `scripts/e2e/PLATFORMS.md` describes it.)
- **FR-023**: The assumption of spec 016 that scenarios with two application processes are out of scope
  MUST be corrected as part of this work, since spec 024 added such scenarios.

**Continuous integration**

- **FR-024**: The new scenarios MUST run in the existing end-to-end job on its stock Linux runner, within
  its time limit, and a failure MUST name the scenario and keep its failure material.

### Key Entities

- **Group**: the set of users, vaults and devices of one scenario, with names, started and ended together.
- **User**: a person of the scenario with exactly one vault; users never share data unless the feature
  under test shares it.
- **Device**: one running application process of a vault, with its own data, ports and screen, addressed by
  name; it can be stopped, restarted, made unreachable and restored.
- **Driver layer**: the platform-independent set of operations scenarios use; implemented once for Linux.
- **Platform document**: the written statement of what each further platform needs.
- **Test relay**: the Nostr relay a group uses for discovery; it can be switched off and on.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: All nine manual checks M1 to M9 have an automatic scenario, and 100 % of the expectations
  written for them in the quickstart of spec 024 are checked by one of them.
- **SC-002**: Over the 20 most recent runs of the end-to-end job on `main`, each of the nine scenarios
  passes in at least 19 of them, and every failure of one of them is listed with its cause. A failure
  caused by the scenario or by the rig, not by the product, has a merged fix before the manual section of
  the quickstart is retired; a failure caused by a defect of the product counts as the scenario doing its
  job and not against its 19. (First worded as 20 runs in a row; with some 40 scenarios and a run per
  merge that measures luck rather than the scenarios, and every flaky unrelated scenario would start the
  count again.)
- **SC-003**: A reviewer who has not seen the helpers writes a scenario with two users with two devices
  each, in which one device is made unreachable and restored, in 60 lines or fewer, using only the
  documentation and the template.
- **SC-004**: The slowest of the nine scenarios ends within 3 minutes on a maintainer's machine, and the
  scenario of M4 ends within 90 seconds even if it has to wait in real time.
- **SC-005**: No scenario or helper of this feature uses a Linux-specific driver, screen, signal or host
  path outside the one Linux implementation of the driver layer (checked by a search that is part of the
  suite's own checks).
- **SC-006**: After any scenario, whether it passed, failed, timed out or was interrupted, no process,
  relay or temporary data of the run is left, and the next run starts at once.
- **SC-007**: The platform document has an entry for each of Windows, macOS, Android and iOS with all four
  parts of FR-022.
- **SC-008**: A deliberately broken sync makes the end-to-end job fail, names the scenario, and keeps the
  failure material of every device that took part.

## Assumptions

- Spec 016's rig is the base: real debug build, per-instance data, ports and virtual screen, a stand-in
  provider where the chat is needed, a test Nostr relay, failure material. This spec extends it and does
  not replace it. The bring-up of the first multi-process scenarios (spec 024, T078) already exists and is
  reused.
- The sync behavior under test is spec 024 as merged; nothing in it is changed by this spec. Where a
  scenario cannot be written because the interface lacks a stable hook (for example an element to read the
  online state), adding that hook is part of this work, following the hook rules of spec 016.
- How the waiting time is controlled (moving the application's clock, a test-only setting, or waiting in
  real time) is a decision for the plan. The user-visible promise stays 60 seconds; the scenario of M4
  must not weaken it.
- A device is made unreachable by restarting its application with no servers and restored by restarting
  it with the group's servers (plan, research R2, decided by the measurement G4); the observable behavior
  is fixed by FR-013. Network namespaces were measured and are possible on a development machine but are
  not adopted.
- Only Linux is implemented. Per further platform, what is probably needed (to be confirmed in each
  follow-up spec): Windows, the Tauri WebDriver bridge with Microsoft's Edge driver and a Windows runner;
  macOS, no official driver for its web view, so a third-party driver or a bridge inside the application,
  and a macOS runner; Android and iOS, a mobile automation server (Appium) with an emulator or simulator
  or real devices, the web view context of the app, and a way for several emulators or simulators to
  reach each other; iOS also needs a macOS runner.
- The ordinary suite stays something a maintainer runs before pushing; the heavier multi-device scenarios
  may be marked so that they can be run on their own, as spec 016 does for the relaunch scenario.
- The machine can run a limited number of application processes at once; building and running all of them
  together has crashed a developer machine before, so the limit in FR-019 is real.
- Own file sync (spec 025), file access and file shares (056), data shares (028), haextensions and the other features that need
  several vaults are not specified here; they add scenarios to the helpers when they are built.
