# Feature Specification: Agent profiles

**Feature Branch**: `083-agent-profiles`

**Created**: 2026-11-05

**Status**: Draft (clarified 2026-11-05: approval mode stays per agent, a registration that expires keeps its
profile, the profile switch from the chat header comes later)

**Input**: User description: "Agent profiles: one place to configure what agents may do, across all domains, for the
whole vault." (operator, 2026-11-05, after the rights list of external agents and the browser rights were reworked in
PRs #817, #825 and #827 and did not feel right: what an agent may do on the web, with files or with the command line
is the user's decision once per kind of access, not once per agent.)

## Background

- **Today every agent has its own rights.** An external agent has its rights on its own page under KI → Agenten
  (spec 021): per topic nothing, reading or changing; the password manager with the entries it reaches; the browser
  with the level for all websites, exceptions and a block list (spec 065); commands; the extensions whose tools it
  sees. The local agent has no such page: its browser and password rights are device settings under KI → Browser.
  An agent that holzi starts itself in the chat on the user's own subscription (spec 032, called "Delegate" in the
  texts today) is registered like an external agent. Files and contacts are granted per agent in their own rows
  (specs 021 US5 and 051).
- **What that costs the user.** Three agents with the same trust need the same ten settings three times. A new agent
  starts with the factory rights and the user walks through every group again. Becoming stricter for a day means
  opening every agent and every group. The same settings exist in two places for the local agent and the others.
- **What the user wants.** Configure what an agent may do once, for the whole vault and across every kind of access
  at once, independent of the model behind it, and change one agent's trust in one step. Stage 1 covers the kinds of
  access in the current rights list; file and contact grants remain per-agent until Stage 2 (Scope).

## Decisions

- **An agent profile is one set of rights over every kind of access at once**, not one set per kind. Switching an
  agent to a stricter profile is one choice, not one choice per kind of access. The kinds of access are those of the
  rights list today: windows and workspaces, chat, settings, models, passwords (with their entries), browser (with
  its websites), commands and the tools of extensions. Files and contacts, and later cloud storage, follow in later
  stages (Scope).
- **There is always the profile "Standard"** (default). It cannot be deleted. Every agent that is not assigned to
  another profile is in it. A new agent starts in it. Its factory rights are those of a new agent today (spec 021
  FR-009): reading in windows, chat and settings; passwords, browser and commands off.
- **An agent is in at most one profile.** The assignment of an agent is one fact, so two profiles for the same agent
  cannot exist, and assigning an agent that is in another profile moves it. The user never has to resolve a conflict.
- **"Agent" is the umbrella word**, with a label for the kind:
  - **Lokaler Agent**: the agent of holzi's own chat, which runs inside holzi on this device (the code calls it the
    built-in agent, `Caller::BuiltinAgent`). It is assigned **per model**, and its model may be one that runs on the
    device or one of a provider (a local model and a cloud model of the local agent can be in different profiles; the
    privacy case: passwords only for local models).
  - **Externer Agent**: a program that connects to holzi over MCP (spec 021). holzi cannot see which model runs
    behind it; the registered identity is what counts, never the name it claims.
  - **Abo-Agent**: an agent program that holzi starts itself in the chat, on the user's own subscription (for
    example Claude Code). It replaces the word "Delegate" / "Delegat" in the user-facing texts of this feature.
- **The name is "Agentenprofil"**, never just "Profil": "Profil" already means the cookies and logins of the agent
  browser.
- **Profiles belong to the vault**, not to a device: they are the same on every device of the vault. An agent is
  registered on one device, but the pages list the agents of every device with the name of their device and assign them
  to a profile from any device.
- **A right a prompt remembers belongs to the profile.** "Für diese Website merken", a remembered fill and a rule for
  a page's requests are written into the profile of the agent that was asked and so apply to every agent in that
  profile. The prompt says so.
- **Guardrails stay out of reach** of every profile and every agent (spec 021 FR-016).
- **Enforcement stays in one place**: what an agent may do is looked up from its profile on every call and every
  list of tools, exactly as it is looked up from its own rights today for external agents and Abo-Agents.
- **The local agent is limited by its profile in every kind of access**, in the first stage too. Today only its
  browser and password fills have rights; its other tools are open and limited by the approval mode alone, so for the
  local agent a profile is a new limit. No model in use loses anything (FR-024); a model that appears later starts
  in "Standard" like every agent.

## Clarifications

### Session 2026-11-05

- Q: Should a profile limit all tools of the local agent in the first stage, or only the browser and the password
  fills? → A: All of them. The local agent's tools were open before (limited only by the approval mode), so for it a
  profile is a new limit; models in use keep full rights through a migrated profile, a model that appears later starts
  in "Standard" and the chat says so.
- Q: How should the profile pages treat agents registered on another device of the vault? → A: Fully: the agents of
  every device are listed with the name of their device, can be assigned to a profile from any device, and are counted
  in every number the pages show. An assignment of an agent whose device is offline applies when that device has synced.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Make an agent stricter in one step (Priority: P1)

The user has three agents in "Standard" and is about to let one of them work on something unknown. They open its page,
pick the agent profile "Vorsichtig" in the one field "Agentenprofil", and from the next call on that agent can only
read, with no browser, passwords or commands, and nothing else changed.

**Why this priority**: it is the reason for the whole feature: changing trust must not mean visiting every kind of
access.

**Independent Test**: create a profile with only reading, assign one agent to it, call tools of three kinds of
access from that agent and from another: the first is refused where the profile gives nothing, the second is
unchanged.

**Acceptance Scenarios**:

1. **Given** an agent in "Standard" with browser and passwords on, **When** the user assigns it to a profile with
   both off, **Then** the agent's next call to a browser or password tool is refused and its tool list no longer
   offers them.
2. **Given** an agent in a profile, **When** the user assigns it to "Standard", **Then** it has the rights of
   "Standard" from the next call.
3. **Given** two agents, **When** the user switches one, **Then** the other keeps its rights.

---

### User Story 2 - Configure once, for every agent (Priority: P1)

The user opens KI → Agentenprofile, opens "Standard" and sets the browser to "Lesen" with a few blocked websites.
Every agent in "Standard", the ones there now and those registered later, has exactly that.

**Why this priority**: this is "once, whatever model ends up using it".

**Independent Test**: change "Standard", then check three agents of it, one of which registers afterwards.

**Acceptance Scenarios**:

1. **Given** three agents in "Standard", **When** the user changes a right of "Standard", **Then** all three have it
   from their next call.
2. **Given** a change that would give agents more than before (for example the browser from off to "Lesen und
   ändern"), **When** the user makes it in "Standard", **Then** the page names how many agents are in the profile
   before the change applies, so a wide change is a conscious one.
3. **Given** the profile page, **When** the user looks at it, **Then** the rights are the same list as an agent's
   rights list is today: one row per topic with the same levels, the password manager and the browser as rows of the
   same list with their entries and exceptions under the row.

---

### User Story 3 - A profile for some agents only (Priority: P2)

The user wants passwords only for local models. They create the profile "Lokal" with passwords on, assign the built-in
agent's local models to it, and leave "Standard" with passwords off. A cloud model of the local agent, and every
external agent, still cannot reach a password.

**Why this priority**: it is the privacy case that makes profiles worth more than a single default, but "Standard"
alone already delivers US1 and US2.

**Independent Test**: create "Lokal" with passwords on, assign one local model; a local model and a cloud model each
ask for a password entry.

**Acceptance Scenarios**:

1. **Given** the profile "Lokal" and a local model in it, **When** the local agent runs with that model, **Then**
   it has the rights of "Lokal"; with a cloud model it has those of "Standard".
2. **Given** an agent in "Lokal", **When** the user assigns it to a third profile, **Then** it is in the third and
   no longer in "Lokal".
3. **Given** the page of a profile, **When** the user adds agents, **Then** an agent that is in another profile is
   shown with that profile's name and moves when chosen; one agent is never in two profiles.
4. **Given** a profile with members, **When** the user deletes it, **Then** its agents are in "Standard" afterwards
   and the user was told how many before.

---

### User Story 4 - A new agent registers into "Standard" (Priority: P2)

An external agent connects for the first time. holzi asks as today, and the question shows what the agent would get:
the rights of "Standard", kind of access by kind of access, and the field "Agentenprofil" to choose another before
registering.

**Why this priority**: registration is the moment the user decides to trust a new program; it must stay a conscious
decision although the rights now come from a profile.

**Independent Test**: connect an unregistered agent; the question lists the rights of "Standard" and offers the
profiles; register with another profile and check the rights.

**Acceptance Scenarios**:

1. **Given** an unregistered agent, **When** it connects, **Then** the question shows the rights it would get and the
   profile it would be in, and registering with a changed choice puts it in that profile.
2. **Given** the user declines, **When** the agent connects again, **Then** it behaves as a declined agent does
   today (spec 021).
3. **Given** an Abo-Agent that holzi starts, **When** it is registered, **Then** the same applies, and the text never
   says "Delegate".

---

### User Story 5 - The local agent has one home for its rights (Priority: P2)

The user finds the rights of the local agent where those of every other agent are: its models appear in the agent
list with the kind "Lokaler Agent", each with its profile. KI → Browser keeps only the choice of the browser and
the cookies and logins of the agent browser.

**Why this priority**: it removes the same settings in two places, which the user asked about.

**Independent Test**: open KI → Agenten and KI → Browser; the local agent's rights are only in the first, and
changing a model's profile there changes what it may do in the browser.

**Acceptance Scenarios**:

1. **Given** the local agent with two models, **When** the user assigns one to a profile, **Then** a chat with
   that model uses the profile and a chat with the other does not.
2. **Given** a model that is no longer installed or offered, **When** the user opens the list, **Then** its
   assignment is still there and can be removed; it applies again when the model returns.
3. **Given** the local agent before the update with its tools open, **When** the update has run, **Then** every
   model it has used is in the profile "Lokaler Agent" and may do what it could; a model added afterwards is in
   "Standard", may only read, and the chat names the profile.

---

### User Story 6 - What a prompt remembers is written into the profile (Priority: P2)

An agent asks to open a website and the user picks "Für diese Website merken". The question says "gilt für alle
Agenten im Agentenprofil Standard". Afterwards every agent in that profile may open the website.

**Why this priority**: without it the profile model would silently diverge from what the user sees.

**Independent Test**: answer a website question with "merken" for agent A of a profile with agent B; B may open the
website without a question; an agent in another profile still asks.

**Acceptance Scenarios**:

1. **Given** two agents in one profile, **When** the user remembers a website for one, **Then** the other has it and
   the profile page lists it.
2. **Given** the same website and an agent in another profile, **When** it opens the website, **Then** it asks as
   before.
3. **Given** a remembered entry, **When** the user removes it on the profile page, **Then** it is gone for every
   agent of the profile.

---

### User Story 7 - Everything that exists today keeps working (Priority: P3)

After updating, every agent has the rights it had before. Agents that had the factory rights are in "Standard". Every
agent with other rights is in a profile named after it, holding those rights. The local agent's browser and
password settings are the rights of its profile.

**Why this priority**: it protects trust (nothing widens, nothing disappears) but happens once.

**Independent Test**: take a vault with agents of different rights, update, and compare what each agent may call
before and after.

**Acceptance Scenarios**:

1. **Given** an agent with the factory rights, **When** the update has run, **Then** it is in "Standard".
2. **Given** an agent with its own rights, **When** the update has run, **Then** it is in a profile with its name and
   may do exactly what it could.
3. **Given** the update ran, **When** it runs again, **Then** nothing changes.

---

### Edge Cases

- **A profile with nothing granted**: allowed; its agents may do nothing, and the page says so.
- **"Standard" widened**: a new agent starts with it, so a generous "Standard" gives an unknown program that much. The
  registration question shows the rights (US4), and the page of "Standard" names its agents (US2).
- **Deleting a profile or an agent**: its agents go to "Standard"; removing an agent removes its assignment.
- **A model or agent that is gone**: the assignment stays until the user removes it (US5).
- **Two devices change the same agent at once**: the agent ends up in one profile, the one of the later change; two
  profiles never remain.
- **A device is offline**: an assignment or a profile change made elsewhere applies on that device as soon as it has
  synced, and until then its agents have what they had.
- **A device on an older version**: it does not sync with a vault whose profiles it does not know, so it never uses
  stale rights; within one version, a profile or assignment this build cannot read counts as no right, never as more.
- **An agent whose registration expired** (spec 021: 90 days without use): it keeps its profile and is asked again
  on its next connection (FR-014).
- **A blocked agent**: stays blocked whatever its profile says.
- **The rights change during a running call**: a call that is running is not cut off; the next call and the next
  list of tools use the new rights.
- **Names**: profiles may share a name (FR-002); the pages tell them apart by their members and rights. The profile
  "Standard" is the one with the fixed id, cannot be renamed or deleted, and is always listed first.

## Requirements _(mandatory)_

### Functional Requirements

**Profiles**

- **FR-001**: The system MUST have a profile "Standard" that cannot be deleted or renamed and that holds every
  agent that is not assigned to another profile.
- **FR-002**: The user MUST be able to create, rename and delete further profiles. A profile is identified by its own
  unique id, never by its name: the same name MAY be used by several profiles if the user wants that, and nothing
  refuses or changes a name because another profile has it.
- **FR-003**: A profile MUST hold the rights of every kind of access of the rights list in one place (Decisions);
  the guardrails MUST NOT be part of it (spec 021 FR-016).
- **FR-004**: The factory rights of "Standard" MUST be those of a new agent today (spec 021 FR-009). A new profile
  starts with nothing granted.
- **FR-005**: Profiles MUST be the same on every device of the vault.

**Assignment**

- **FR-006**: An agent MUST be in exactly one profile at any time, and assigning it to another MUST move it; no
  action of the user and no concurrent change on two devices may leave it in two.
- **FR-007**: The local agent MUST be assigned per model; an external agent and an Abo-Agent per registered
  identity. The list of agents MUST show each with its kind (Lokaler Agent, Externer Agent, Abo-Agent) and, for a
  registered agent, the name of the device it is registered on, and MUST include the agents of every device of the vault.
- **FR-008**: An assignment MUST survive the absence of its model or agent until the user removes it.
- **FR-009**: Deleting a profile MUST move its agents to "Standard" and MUST tell the user how many that are before
  it is done.

**Enforcement**

- **FR-010**: What an agent may do MUST be looked up from the rights of its profile on every call and every list of
  tools, in the one place that decides it today for external agents, so a tool an agent does not see is a tool it
  cannot call. The local agent MUST be limited the same way, in every kind of access of the rights list, from the first stage on.
- **FR-011**: A change of a profile or of an assignment MUST apply to the next call and the next list of tools of the
  agents concerned, on every device, without restarting an agent.
- **FR-012**: A device that cannot read a profile MUST refuse the rights it cannot read, never fall back to wider
  ones.

**Registration and prompts**

- **FR-013**: The registration question MUST show the rights the agent would get kind of access by kind of access,
  the profile it would be in, and a field to choose another profile before registering. A new agent MUST start in
  "Standard" unless the user chooses otherwise.
- **FR-014**: When an agent's registration expires (spec 021), its assignment MUST stay: only the registration
  expires, the agent is asked again on its next connection and keeps its profile.
- **FR-015**: A right remembered from a prompt (a website, a fill, a rule for a page's requests) MUST be written into
  the profile of the agent asked, and the prompt MUST say it applies to every agent in that profile, naming it.
- **FR-016**: Where the user can change a profile, the page MUST show how many agents are in it, counting the agents
  of every device of the vault (for "Standard" those of every device that are in no other profile), and a change that
  widens it MUST name that number before it applies.
- **FR-017**: The approval mode of an agent (Manual, Auto, Plan) MUST stay a setting of the agent, as today: it says
  how an agent asks, not what it may do, and is not part of a profile.

**Pages and texts**

- **FR-018**: KI → Agentenprofile MUST list the profiles with their agent counts, open one to edit its rights and
  members, and offer creating, renaming and deleting.
- **FR-019**: The page of an agent MUST show its profile in one field "Agentenprofil" that changes it, and MUST show
  the effective rights read-only with the name of the profile they come from.
- **FR-020**: The editor of a profile MUST reuse the rights list of an agent today: one row per topic with the same
  levels, the password manager and the browser as rows of that list with their entries and exceptions as an inset
  block under the row.
- **FR-021**: KI → Browser MUST keep only the choice of the browser, its trackers and the profile (cookies and logins)
  of the agent browser; the rights of the local agent MUST NOT be there any more.
- **FR-022**: The user-facing texts of the agent pages, the registration question, the profile pages and the chat
  notices about an agent's rights MUST say "Abo-Agent" and never "Delegate" or "Delegat"; "Profil" alone MUST NOT be
  used for an agent profile.
- **FR-023**: The user MUST be able to switch an agent's profile on the agent's page and on the profile page. A
  switch from the chat header is not part of the first stage.

**Migration**

- **FR-024**: On the first start after the update every agent MUST keep exactly the rights it had: an agent with the
  factory rights joins "Standard", every other agent gets a profile with its name holding its rights. For the built-in
  agent the migration MUST create one profile "Lokaler Agent" with its browser and password settings, full rights in
  every other kind of access it could use, and assign every model it has used to it, so no model in use is narrowed.
- **FR-025**: The migration MUST NOT give any agent more than it had, and running it again MUST change nothing.
- **FR-026**: A model of the local agent that is in no profile MUST be in "Standard", and when the active model's
  profile gives less than the chat asked for, the chat MUST say which profile the model is in and what it may do; the
  approval mode of the local agent stays as it is (FR-017).

### Key Entities

- **Agentenprofil**: a named set of rights over every kind of access (the topics, passwords with their entries,
  browser with its websites and rules, commands, extensions), the same in the whole vault. "Standard" always exists.
- **Agent**: whoever acts on holzi: a model of the local agent, an external agent or an Abo-Agent, with a kind, a
  name and, for a registered agent, the device it is registered on.
- **Zuordnung**: the one fact which profile an agent is in. At most one per agent.
- **Remembered answer**: a website, fill or request rule the user chose to remember; it is a right of a profile.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: The user can make an agent stricter in at most three steps (open the agent, choose a profile, done) and
  the agent is limited from its next call across every kind of access covered by Stage 1. File and contact access
  remain governed by their existing per-agent grants until Stage 2.
- **SC-002**: A right changed in a profile reaches every agent in it from its next call, with no restart, and an
  agent in another profile is unchanged.
- **SC-003**: Across any sequence of assignments, deletions and concurrent changes, no agent is ever in two profiles.
- **SC-004**: After the update, 100 % of agents may do exactly what they could before, none more.
- **SC-005**: A user can find the rights of the local agent and of an external agent in the same place; no right
  can be set in two places.
- **SC-006**: In a test with a local model in a profile with passwords and a cloud model in "Standard", the cloud
  model never gets a password entry.

## Assumptions

- The rights of the local agent today (device settings) and of external agents (rows per agent) are the
  inputs of the migration; their meaning does not change, only where they live.
- The existing registration (name, claimed identity, 90 days without use, blocking) stays as spec 021 defines it;
  this spec changes where rights come from, not who is registered.
- Approval modes stay per agent (FR-017); the guardrails stay as they are.
- A user has a handful of profiles, not hundreds; no search or grouping of profiles is needed.
- The profile page and the agent page are used on the desktop first; the phone shows the same lists.

## Scope

- **Stage 1 (this spec's first delivery)**: the profile model and its enforcement for the kinds of access of the
  rights list today (windows and workspaces, chat, settings, models, passwords, browser, commands, extensions);
  KI → Agentenprofile; the field "Agentenprofil" on each agent's page and in the registration question; the move of
  the local agent's rights; the renaming to "Abo-Agent" in the texts it touches; the migration. File and contact
  grants stay as they are today, per agent, and are not changed by a profile or profile switch in this stage.
- **Stage 2**: files and contacts (today per-agent rows with storages and folders) become part of a profile; their
  existing per-agent grants are migrated without changing what each agent may do.
- **Stage 3**: cloud storage and further kinds of access.
- **Out of scope**: rights per time of day or per conversation; profiles that inherit from each other; grouping
  models ("all local models") as one assignment; a profile for a person instead of an agent.
