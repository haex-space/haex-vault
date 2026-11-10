# Feature Specification: Agent profiles

**Feature Branch**: `083-agent-profiles`

**Created**: 2026-11-05

**Status**: Draft (clarified 2026-11-05: a registration that expires keeps its profile, the profile switch from the
chat header comes later; revised 2026-11-10: an external agent has no approval mode of its own, and the page "Agenten
und Modelle" is gone, see FR-017 and FR-035)

**Input**: User description: "Agent profiles: one place to configure what agents may do, across all domains, for the
whole vault." (operator, 2026-11-05, after the rights list of external agents and the browser rights were reworked in
PRs #817, #825 and #827 and did not feel right: what an agent may do on the web, with files or with the command line
is the user's decision once per kind of access, not once per agent.)

## Background

- **Today every agent has its own rights.** An external agent has its rights on its own page under KI → Externe Agenten
  (spec 021, renamed "Agenten und Modelle" by PR M2, gone since 2026-11-10: FR-035): per topic nothing, reading or changing; the password manager with the entries it reaches; the browser
  with the level for all websites, exceptions and a block list (spec 065); commands; the extensions whose tools it
  sees. The local agent has no such page: its browser and password rights are device settings under KI → Browser.
  An agent that holzi starts itself in the chat on the user's own subscription (spec 032, called "Delegate" in the
  texts today) is registered like an external agent. Files and contacts are granted per agent in their own rows
  (specs 021 US5 and 051); the chat's models all shared the one agent "builtin", so there they could not differ by model (PR K).
- **What that costs the user.** Three agents with the same trust need the same ten settings three times. A new agent
  starts with the factory rights and the user walks through every group again. Becoming stricter for a day means
  opening every agent and every group. The same settings exist in two places for the local agent and the others.
- **What the user wants.** Configure what an agent may do once, for the whole vault and across every kind of access
  at once, independent of the model behind it, and change one agent's trust in one step. Stage 1 covers the kinds of
  access in the current rights list; file and contact grants follow in Stage 2 (Scope, PR K).

## Decisions

- **An agent profile is one set of rights over every kind of access at once**, not one set per kind. Switching an
  agent to a stricter profile is one choice, not one choice per kind of access. The kinds of access are those of the
  rights list today: windows and workspaces, chat, settings, models, passwords (with their entries), browser (with
  its websites), commands and the tools of extensions. Contacts and files follow in PR K (FR-032, FR-033), later cloud
  storage (Scope).
- **There is always the profile "Standard"** (default). It cannot be deleted. Every agent and every model is in it, always,
  and it applies whenever nothing else does (see "Several profiles, one active"). A new agent starts with it. Its factory rights are those of a new agent today (spec 021
  FR-009): reading in windows, chat and settings; passwords, browser and commands off.
- **Several profiles, one active** (revised 2026-11-06 after the first manual check). An agent or model is not moved
  from one profile to another: it can be a member of several profiles at once, for example a "sensibel" and an "offen"
  one, and the user uses the same model in both depending on what they are doing. Exactly one profile is **active** for
  the whole vault at any time, and the user sets which. An agent has the rights of the active profile when it is a member
  of it, and those of "Standard" when it is not. "Standard" may itself be the active profile; then every agent has the
  rights of "Standard". The active profile is one value of the vault, synced to every device. At any time exactly one set
  of rights applies to an agent, so the user never has to resolve a conflict between profiles.
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
- **Abo-Agents and the local agent are one kind: a model** (decided 2026-11-07, PR J, amends FR-007, FR-006b and
  FR-017): a model of an Abo-Agent's provider (for example "Claude Opus" of Claude Code) is listed, assigned to profiles,
  offered in the chat and limited by its profile exactly like a model of a local or API provider. There is no separate
  registration and no separate row per vendor and device for rights; if the user wants Abo-Agents treated differently from
  local models, they make two profiles ("Lokal", "Abo") and put the models into them. The autonomy mode and the deny rules
  of Abo-Agents (specs 009, 032) become rights of a profile, and the device-wide setting today is the starting value of
  "Standard". External agents (programs that connect to holzi) stay agents of their own, registered per identity.
- **Guardrails stay out of reach** of every profile and every agent (spec 021 FR-016).
- **Enforcement stays in one place**: what an agent may do is looked up from its profile on every call and every
  list of tools, exactly as it is looked up from its own rights today for external agents and Abo-Agents.
- **The local agent is limited by its profile in every kind of access**, in the first stage too. Today only its
  browser and password fills have rights; its other tools are open and limited by the approval mode alone, so for the
  local agent a profile is a new limit. Every model starts in "Standard" like every agent (FR-024, FR-026).
- **A profile belongs to many agents, and an agent to many profiles.** A profile holds the rights for every kind of access
  and names the agents and models it applies to (its members); an agent or model can be a member of any number of
  profiles. Adding it to one never removes it from another.
- **There is no migration.** What agents were allowed before this feature, and the local agent's old browser and
  password settings, are not converted: they stop counting and are removed, and the user sets profiles anew. Who is
  registered (name, credential, blocks, approval mode) is not a right and stays.

## Clarifications

### Session 2026-11-05

- Q: Should a profile limit all tools of the local agent in the first stage, or only the browser and the password
  fills? → A: All of them. The local agent's tools were open before (limited only by the approval mode), so for it a
  profile is a new limit; every model starts in "Standard" and the chat says so (revised 2026-11-06: there is no
  migration that would give models in use a profile of their own).
- Q: How should the profile pages treat agents registered on another device of the vault? → A: Fully: the agents of
  every device are listed with the name of their device, can be assigned to a profile from any device, and are counted
  in every number the pages show. An assignment of an agent whose device is offline applies when that device has synced.

### Session 2026-11-06

- Q: An agent in several profiles? (2026-11-06, after the first manual check) → A: Yes. A sensitive and an open profile
  with the same model, chosen by what the user is doing. Agents are not moved between profiles. Exactly one profile is
  active for the whole vault, set by the user from KI → Agentenprofile; an agent that is not a member of it has the
  rights of "Standard". "Standard" applies to all agents. The switch for extensions is always usable (FR-020a).
- Q: Should the rights agents have today be carried into profiles? → A: No. They are not migrated: after the update
  every agent and every model of the local agent is in "Standard" and has its factory rights, and the user sets the
  rights anew in profiles. Old per-agent rights and the local agent's old browser and password settings are removed.
  A profile belongs to many agents and names which agents and models it applies to.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Make an agent stricter in one step (Priority: P1)

The user has three agents in "Standard" and is about to let one of them work on something unknown. They open its page,
add it to the agent profile "Vorsichtig" and make "Vorsichtig" the active agent profile, and from the next call on
that agent can only read, with no browser, passwords or commands. Agents that are not members of "Vorsichtig" keep the
rights of "Standard".

**Why this priority**: it is the reason for the whole feature: changing trust must not mean visiting every kind of
access.

**Independent Test**: create a profile with only reading, add one agent to it and make it active, call tools of three
kinds of access from that agent and from another: the first is refused where the profile gives nothing, the second has
the rights of "Standard".

**Acceptance Scenarios**:

1. **Given** an agent with browser and passwords on in "Standard", **When** the user adds it to a profile with both off
   and makes that profile active, **Then** the agent's next call to a browser or password tool is refused and its tool
   list no longer offers them.
2. **Given** that profile active, **When** the user makes "Standard" active, **Then** the agent has the rights of
   "Standard" from the next call, and it is still a member of the other profile.
3. **Given** two agents of which only one is a member of the active profile, **Then** the other has the rights of
   "Standard".

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

1. **Given** the profile "Lokal" is active and a local model is a member of it, **When** the local agent runs with that
   model, **Then** it has the rights of "Lokal"; with a cloud model, which is no member, it has those of "Standard".
2. **Given** an agent in "Lokal", **When** the user adds it to a third profile, **Then** it is a member of both, and
   the profile that is active decides which rights apply.
3. **Given** the page of a profile, **When** the user adds agents, **Then** an agent that is a member of other profiles
   stays in them, and the list shows them; removing it from this profile does not touch the others.
4. **Given** two profiles "sensibel" and "offen" with the same model in both, **When** the user makes the other one
   active, **Then** the same model has the other rights from its next call.
5. **Given** a profile with members, **When** the user deletes it, **Then** its agents are in "Standard" afterwards
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
list with the kind "Lokaler Agent", each with its profiles. KI → Browser keeps only the choice of the browser and
the cookies and logins of the agent browser.

**Why this priority**: it removes the same settings in two places, which the user asked about.

**Independent Test**: open KI → Agentenprofile and KI → Browser; the local agent's rights are only in the first, and
adding a model to a profile and making that profile active there changes what it may do in the browser.

**Acceptance Scenarios**:

1. **Given** the local agent with two models and an active profile, **When** the user adds one to it, **Then** a chat
   with that model uses the profile and a chat with the other has the rights of "Standard".
2. **Given** a model that is no longer installed or offered, **When** the user opens the list, **Then** its
   memberships are still there and can be removed; they apply again when the model returns.
3. **Given** the update ran, **When** the user starts a chat with any model of the local agent, **Then** the model has
   the rights of "Standard", may only read, and the chat names the profile; the user gives it more by adding it to
   another profile and making that one active.

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

### User Story 7 - The update resets what agents may do (Priority: P3)

After the update every agent and every model of the local agent is in "Standard" and has its factory rights. What an
agent was allowed before, and the local agent's old browser and password settings, no longer count. Nothing is
converted; the user sets profiles anew. Who is registered stays.

**Why this priority**: it keeps the update simple and the result predictable, but it happens once.

**Independent Test**: take a vault with agents of different rights, update, and check what each agent may call.

**Acceptance Scenarios**:

1. **Given** an agent that had its own rights, **When** the update has run, **Then** it is in "Standard", has the
   factory rights and is still registered with the same credential.
2. **Given** the local agent with browser and password settings, **When** the update has run, **Then** those settings are
   gone and its models are in "Standard".
3. **Given** the update ran, **When** the app starts again, **Then** nothing is reset a second time.

---

### Edge Cases

- **A profile with nothing granted**: allowed; its agents may do nothing, and the page says so.
- **"Standard" widened**: a new agent starts with it, so a generous "Standard" gives an unknown program that much. The
  registration question shows the rights (US4), and the page of "Standard" names its agents (US2).
- **Deleting a profile or an agent**: deleting a profile removes its memberships; when it was the active one, "Standard"
  becomes active. Removing an agent removes all its memberships.
- **An agent that is a member of no profile but "Standard"**: has the rights of "Standard" whatever profile is active.
- **A model or agent that is gone**: the assignment stays until the user removes it (US5).
- **Two devices change memberships at once**: memberships of one agent in different profiles are different rows and all
  stay; two devices that add the same membership write the same row. **Two devices set the active profile at once**: the
  later change wins; one profile is active.
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
  agent and model, whatever other profiles it is a member of.
- **FR-002**: The user MUST be able to create, rename and delete further profiles. A profile is identified by its own
  unique id, never by its name: the same name MAY be used by several profiles if the user wants that, and nothing
  refuses or changes a name because another profile has it.
- **FR-003**: A profile MUST hold the rights of every kind of access of the rights list in one place (Decisions);
  the guardrails MUST NOT be part of it (spec 021 FR-016).
- **FR-004**: The factory rights of "Standard" MUST be those of a new agent today (spec 021 FR-009). A new profile
  starts with nothing granted.
- **FR-005**: Profiles MUST be the same on every device of the vault.

**Assignment**

- **FR-006**: An agent MUST be able to be a member of any number of profiles at once; adding it to a profile MUST NOT
  remove it from another. Exactly one profile MUST be active for the vault at any time (FR-006a), so exactly one set of
  rights applies to an agent, and no action of the user and no concurrent change on two devices may leave an agent
  with two.
- **FR-006a**: The user MUST be able to set which profile is active, from KI → Agentenprofile (on the list of profiles,
  where they are created, and on the page of a profile). The active profile MUST be marked in the list. An agent has the
  rights of the active profile when it is a member of it, else those of "Standard". Making a profile active applies at
  once, with no question (FR-016). No agent can change the active profile (spec 021 FR-016).
- **FR-006b**: The chat MUST offer only the models of the active profile and refuse to use any other (decided
  2026-11-07). While "Standard" is active that is every model except those the user took out of it; "Standard" holds
  every model until then, also ones downloaded later, and the user MUST be able to take a model out of "Standard" and put
  it back (on its own page and on the page of "Standard"). While another profile is active it is only the models that are
  members of it. A turn with a model that is not offered MUST fail before anything is stored, with a message that names
  the active profile as the cause, for a model of an Abo-Agent's provider too (PR J; before it, Abo-Agents were exempt). The
  agents of "Standard" cannot be taken out of it.
- **FR-006c**: The chat MUST have a select "Agentenprofil" next to the model select, which shows and changes the active
  profile (the same one as FR-006a). It MUST NOT be shown while "Standard" is the only profile.
- **FR-007**: The local agent MUST be assigned per model, an Abo-Agent's model too (FR-027); an external agent per registered
  identity. The list of agents MUST show each with its kind (Lokaler Agent, Externer Agent) and, for a
  registered agent, the name of the device it is registered on, and MUST include the agents of every device of the vault.
  Every model of the local provider MUST be listed as a row of its own with its profile, as soon as it was downloaded or
  imported on any device of the vault, whatever the state of its file on this device (it is not required to be active or
  installed here: the user may switch between several); the models of provider APIs may be offered in a pick list.
- **FR-008**: An assignment MUST survive the absence of its model or agent until the user removes it.
- **FR-009**: Deleting a profile MUST remove its memberships, MUST make "Standard" active when it was the active one,
  and MUST tell the user how many agents are members before
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

- **FR-013**: The registration question MUST show the rights the agent would get kind of access by kind of access
  (those of the active profile if the agent were a member of it, else those of "Standard"), and a choice of a profile
  to make it a member of before registering (more are added on the agent's page). A new agent MUST be a member of no profile but "Standard" unless the user
  chooses otherwise.
- **FR-014**: When an agent's registration expires (spec 021), its memberships MUST stay: only the registration
  expires, the agent is asked again on its next connection and keeps its profile.
- **FR-015**: A right remembered from a prompt (a website, a fill, a rule for a page's requests) MUST be written into
  the profile that applies to the agent asked (the active one when it is a member of it, else "Standard"). The prompt
  MUST NOT say that it applies to every agent in that profile or how many that are: that a right given to an agent
  gives it more is obvious (decided 2026-11-08, with FR-016). The window still refuses an answer when the profile
  changed while the question was open.
- **FR-016**: Where the user can change a profile, the page MUST show how many agents are in it, counting the agents
  of every device of the vault (for "Standard" every agent and every model that did not leave it). A change of the
  rights of a profile, and making a profile active, MUST NOT ask for a confirmation, however much it gives: an agent that
  gets a right can obviously do more, and the switch is the decision (decided 2026-11-07, which widens the decision of
  2026-11-06 for extensions and commands to every right). The change applies at once.
- **FR-017** (revised 2026-11-10): An external agent MUST NOT have an approval mode the user can set: it always runs in
  Auto, so reading and changing run without a question, and holzi asks before a destroying call and before a secret
  (spec 021 FR-021). holzi is an MCP server like any other: the profile decides what an agent may do, and how the
  agent's own client asks is the client's business. The mode is neither part of a profile nor of the registration
  question. The approval mode of the chat, which a model of an Abo-Agent follows, is unchanged (FR-030).

**Pages and texts**

- **FR-018**: KI → Agentenprofile MUST list the profiles with their agent counts, open one to edit its rights and
  members, and offer creating (a name field with a "+" button), renaming, deleting and making a profile active (FR-006a). The page MUST NOT explain that a
  name may repeat or that a new profile starts without rights; the introduction says that "Standard" applies to all agents
  (external, Abo-Agent and local) and that exactly one profile applies to an agent at any time.
- **FR-019** (revised 2026-11-10): There is no page of a single agent, model or provider (FR-035). The page of a profile
  MUST list its members, agents, models and providers, each with its kind and, for an agent, its device, and MUST NOT
  show the rights of a single member: they depend on which profile is active and are those of the profile's page.
- **FR-020a**: The switch "Alle Erweiterungen" MUST always be usable, also while no extension is installed: the user may
  configure it before installing one, and it covers extensions installed later.
- **FR-020**: The editor of a profile MUST reuse the rights list of an agent today: one row per topic with the same
  levels, the password manager and the browser as rows of that list with their entries and exceptions as an inset
  block under the row.
- **FR-020b**: Removing an agent from a profile on the profile's page MUST be one bin icon in its row (the one
  `DeleteButton` every other "delete this" uses), never a text button.
- **FR-021**: KI → Browser MUST keep only the choice of the browser, its trackers and the profile (cookies and logins)
  of the agent browser; the rights of the local agent MUST NOT be there any more.
- **FR-022**: The user-facing texts of the agent pages, the registration question, the profile pages and the chat
  notices about an agent's rights MUST say "Abo-Agent" and never "Delegate" or "Delegat"; "Profil" alone MUST NOT be
  used for an agent profile.
- **FR-023** (revised 2026-11-10): The user MUST be able to add an agent, a model or a provider to a profile and remove it
  from it on the profile's page, and to make a profile active there and on the list of profiles. A switch of the active
  profile from the chat header is not part of this stage.

**Update**

- **FR-024**: After the update no right an agent had before agent profiles, and no browser or password setting of the
  local agent, MUST count any more; every agent and every model of the local agent MUST have the rights of "Standard"
  until the user adds it to another profile and makes that one active. Registrations (name, credential, blocks) MUST stay. The old rights MAY be
  deleted.
- **FR-025**: The reset MUST NOT be applied again on a later start, and nothing the user has set in agent profiles
  since MUST be undone by it.
- **FR-026**: A model of the local agent that is a member of no profile MUST have the rights of "Standard", and when the
  profile that applies to the active model gives less than the chat asked for, the chat MUST say which profile that is
  and what the model may do; the
  approval mode of the local agent stays as it is (FR-017).

**One kind for Abo-Agents and local models (PR J, decided 2026-11-07)**

- **FR-027**: An Abo-Agent's model MUST be a subject of the kind "model" (`model:<provider id>:<remote id>`): the pages list
  it under "Lokaler Agent" with the others, it is a member of profiles and of "Standard", and the chat offers it by FR-006b.
  The rows of an Abo-Agent per vendor and device (`ClaudeCodeDelegate`, `CodexDelegate`) MUST NOT carry rights any more.
- **FR-028**: When an Abo-Agent calls holzi (over MCP, through the pass of its start) the rights MUST be those of the model
  the start runs with, resolved on every call and every list of tools like for any model, so the same profile limits the
  same tools whichever kind of model answers. The pass names the model; a call of a pass whose model is not offered by the
  active profile is refused.
- **FR-029**: Holzi's tools MUST be offered to an Abo-Agent without a registration question: what its model may do is the
  profile. "Rechte festlegen" and the registration dialog for Abo-Agents MUST be removed, as MUST the status "not
  registered" and "expired" of an Abo-Agent in the chat; what remains is "MCP access is off" (no Abo-Agent gets holzi's
  tools) and the rights of the profile (a model with no rights gets no tool).
- **FR-030**: The autonomy mode (ungated, gated) and the deny rules of Abo-Agents MUST be rights of a profile, shown in its
  editor and resolved for the model of the start like every other right; a profile that grants none of them behaves like
  today's factory setting. The device preference `chat.autonomy_mode` and the deny list are migrated into "Standard" and
  profiles assigned to existing Abo-Agent models when those profiles have no delegate group, then removed so there is no
  second source. A synced vault marker prevents stale device preferences from applying the conversion more than once. A
  profile without the group asks for every call (autonomy "standard"); existing restrictions are preserved without
  replacing a user's profile choice.
  A group this build cannot read (an unknown autonomy, an unknown deny category) counts as
  absent, never as a wider one. "Standard" without a row of its own has the factory value, "ungated" (today's default).
  The chat reads the right for the model of the turn when it sends and the window sends no autonomy mode any more. The pages
  "Autonomie für Abo-Agenten" and "Verbotsregeln für Abo-Agenten" under KI are gone: the group "Abo-Agenten" is in the editor
  of a profile. The approval mode (Manual, Auto, Plan) stays a setting of the chat (FR-017).
- **FR-031**: The update MUST convert what exists: the memberships of a registered Abo-Agent row become memberships of every
  model of its provider (an ordinary delete of the row's memberships, so every device converges), its row stays as a technical
  identity that no page lists, and nothing the user set is widened (a model that was in no profile is in "Standard", as before).

**Contacts and files in the profile (PR K, decided 2026-11-08)**

- **FR-032**: What an agent reaches in the address book MUST be the group `contacts` of the profile that applies to it: a
  level for all contacts and a level for single contact groups, each `read`, `readWrite` or `denied`, resolved for the
  subject like every other right (a model of the local agent, an external agent, an Abo-Agent's model). Without an entry
  the agent is asked at its first access as before (spec 051 FR-030); "Immer erlauben" and "Ablehnen" are written into the
  profile that applies, and when another profile applies by the time the question is answered the answer holds for that call
  only. A level this build cannot read counts as `denied`. The rows of `agent_file_permissions` of kind `contacts` are not
  read any more. The editor of a profile shows the group "Kontakte"; the agent page no longer lists contacts.
- **FR-033**: The same MUST hold for files (PR K2): the files of the device (always this device's, whichever device the profile
  is read on), each storage (another own device and a received share count as one), encrypted folders with the models they
  reach (local only, or cloud models too), and the rights to manage file shares and sync rules are the group `files` of the
  profile, so two profiles can give different file access to different models. The files of the device are **off** in the
  factory rights of "Standard" for every agent, the chat's agent included (decided 2026-11-08; before, only the chat's agent
  had them). A storage or an encrypted folder without an entry asks at the first access; "Immer erlauben" and a kept answer
  are written into the profile that applies, and when another profile applies by the time the question is answered the answer
  holds for that call only. A level this build cannot read counts as `denied`. Removing a storage removes its entries from
  every profile. Only the user sets these rights (guardrails); an agent can exercise a right its profile grants, also managing
  file shares and sync rules. The table `agent_file_permissions` is dropped (migration 0059, no conversion); the page KI →
  Dateien and the files part of the agent page are gone, the editor of a profile shows the group "Dateien".

**Providers as members (PR M, decided 2026-11-08)**

- **FR-034**: A provider of models (the local one, an API provider, an Abo-Agent's provider) MUST be a subject of the kind
  "provider" (`provider:<provider id>`) that can be a member of profiles and leave "Standard" like a model. Every model of the
  provider, one that comes later too, counts as a member wherever the provider is, in addition to the model's own memberships
  (they add up, so a model can be in a profile through its provider and in another of its own). The chat offers a model while
  "Standard" is active unless the model or its provider left it, and otherwise when the model or its provider is a member of
  the active profile; the rights of a model are those of the profile that applies to it by that rule. This lets the user give
  the LiteLLM of an employer other rights than a private OpenRouter account, though the same model runs behind both. The
  lists group the models by provider (PR L); the profile's page lists a provider as one member that stands for all its models and
  shows a model that is in the profile only through its provider without a bin of its own. A member count counts agents and models, not providers.
  A membership that cannot be read, on the model or on its provider, gives no right (FR-012).
- **FR-035** (revised 2026-11-10): The settings MUST NOT have a page of "external agents" or of "agents and models" next
  to the agent profiles; there are no pages for a single agent, a single model or a provider either. The memberships of
  agents, models and providers are kept on the profile's page only (FR-019, FR-023), where "Standard" also takes a model
  out and back in. KI → "Externer Zugang" (`ai.access`) has the switch of the MCP access on this device, its state, the
  setup snippets for Claude Code, Codex and other MCP clients, and the external agents that registered on this device:
  each with its name, whether it is connected, its agent profile and when it registered and last connected, and with
  "Protokoll" (its log, `ai.access.log`) and "Widerrufen"; a blocked agent has "Sperre aufheben". The rights an agent has
  are shown only on its profile's page (and in the registration question), because they depend on which profile is
  active. The approval mode of the chat agent is the autonomy of its profile (FR-030).

### Key Entities

- **Agentenprofil**: a named set of rights over every kind of access (the topics, passwords with their entries,
  browser with its websites and rules, commands, extensions), the same in the whole vault, with the agents and models it applies to (its members). "Standard" always exists.
- **Agent**: whoever acts on holzi: a model of the local agent, an external agent or an Abo-Agent, with a kind, a
  name and, for a registered agent, the device it is registered on.
- **Zugehörigkeit**: the fact that an agent is a member of a profile. Any number per agent, one row each.
- **Aktives Agentenprofil**: the one profile that applies to the vault right now; "Standard" until the user sets another.
- **Remembered answer**: a website, fill or request rule the user chose to remember; it is a right of a profile.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: The user can make an agent stricter in at most three steps (open the agent, choose a profile, done) and
  the agent is limited from its next call across every kind of access (contacts and files, PR K).
- **SC-002**: A right changed in a profile reaches every agent in it from its next call, with no restart, and an
  agent in another profile is unchanged.
- **SC-003**: Across any sequence of memberships, activations, deletions and concurrent changes, exactly one profile is
  active, and exactly one set of rights applies to an agent at any time.
- **SC-004**: After the update, 100 % of agents have exactly the rights of "Standard" and no right from before the update.
- **SC-005**: A user can find the rights of the local agent and of an external agent in the same place; no right
  can be set in two places.
- **SC-006**: In a test with a local model in a profile with passwords and a cloud model in "Standard", the cloud
  model never gets a password entry.

## Assumptions

- The rights of the local agent today (device settings) and of external agents (rows per agent) are not carried over
  (operator, 2026-11-06). An agent that was more restricted than "Standard" before gets the factory rights until the
  user sets a profile; the user accepts that.
- The existing registration (name, claimed identity, 90 days without use, blocking) stays as spec 021 defines it;
  this spec changes where rights come from, not who is registered.
- Approval modes stay per agent (FR-017); the guardrails stay as they are.
- A user has a handful of profiles, not hundreds; no search or grouping of profiles is needed.
- The profile page and the agent page are used on the desktop first; the phone shows the same lists.

## Scope

- **Stage 1 (this spec's first delivery)**: the profile model and its enforcement for the kinds of access of the
  rights list today (windows and workspaces, chat, settings, models, passwords, browser, commands, extensions);
  KI → Agentenprofile; the field "Agentenprofil" on each agent's page and in the registration question; the move of
  the local agent's rights; the renaming to "Abo-Agent" in the texts it touches; the reset of the old rights (no migration). File and contact
  grants stay as they are today, per agent, and are not changed by a profile or profile switch in this stage.
- **Stage 2 (PR K, decided 2026-11-08, built)**: contacts (K1) and files, with sharing and sync rules (K2) become part of a profile;
  the existing per-agent grants are not carried over (there are no users to convert), they are removed when the stage lands.
  The vault should be controllable by agents as far as the user allows: managing file shares and sync rules are rights a
  profile can grant too, and only the user sets a profile.
- **Stage 3**: cloud storage and further kinds of access.
- **Out of scope**: rights per time of day or per conversation; profiles that inherit from each other; grouping
  models ("all local models") as one assignment; a profile for a person instead of an agent.

## Amendment 2026-11-10 (pages of agents and models removed)

Decided after a hand test: the pages "KI → Agenten und Modelle", one agent, one model and one provider showed the
memberships a second time and a read-only copy of the rights that depend on the active profile, which confused. The
memberships are kept on the profile's page only (FR-019, FR-023, FR-035); the external agents with their log and
revocation are on "KI → Externer Zugang". An external agent has no approval mode of its own (FR-017; spec 021 FR-021).
Where the stories and tasks above send the user to an agent's or a model's page, read: the profile's page.
