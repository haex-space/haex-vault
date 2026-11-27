# Feature Specification: Delegates reach the web only through HaexVault's agent browser

**Feature Branch**: `102-delegate-web-tools`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Every subscription delegate (Claude Code, Codex, Abacus AI) reaches the web only through
HaexVault's own agent browser, never through its own web tools or its own browser. Today the delegate instruction only asks
for that. The own web tools are blocked always, in every autonomy mode and also when the device's MCP access is off; network
access through the delegate's shell is named as a residual for the later OS sandbox. Operator rules: never silent, no host
CLIs, no backward compatibility, new identifiers use the haex prefix. Out of scope: an OS sandbox, blocking shell network,
HaexVault's own agent browser itself, the built-in chat models. A real run per vendor needs the operator's accounts and is an
operator task." (operator, 2026-11-27)

## Background

- **What a delegate is told today.** A delegate in the chat gets HaexVault's tools through its MCP server `haex` (spec 021
  US6) together with an instruction: research on the web, searching a shop or opening a website goes through HaexVault's
  agent browser (`browser_search`, `browser_open`, `browser_follow_link`, `browser_page_read`), not the delegate's own web
  fetch or web search. That is a request to the model, not a rule the program enforces.
- **Why delegates often have no HaexVault tools today.** Delegates reach HaexVault's tools through the same MCP server as
  programs outside HaexVault (an editor such as VS Code), and that server only runs while the device's external access
  (Settings → AI → External access) is on. With it off, a delegate HaexVault starts itself gets neither HaexVault's tools
  nor the instruction, even when the agent profile of its model allows the browser.
- **What was observed (2026-11-27).** The external access was off, so Abacus had no `haex` server and no instruction,
  although the profile allowed "Browser: read and change". Asked to open a website, it wanted to start its own bundled
  browser through its shell, with a browser profile under `/tmp/ab-profile`, that is outside the vault's folder. Under
  "Standard" the person was asked and could decline; under "Ungated" it would have run without a question. Claude Code and
  Codex have the same kind of own web tools; Codex also has its own browser features, and Claude Code can drive the
  person's Chrome ("Claude in Chrome").
- **Why it matters.** HaexVault's agent browser runs inside the vault's isolation, under the agent profile's rights
  (spec 083) and with its own approvals before buying, paying and sending. A delegate's own web tools and browser bypass all
  of that: they keep cookies and profiles where HaexVault does not look, act on websites without HaexVault's approvals, and
  bring web content into the conversation that spec 097 treats as foreign.
- **What the programs offer** (measured with the vault's own versions on 2026-11-27, research R1–R4): each of the three
  programs can be started with its own browser switched off or denied by rule, also in its own full-autonomy mode; Claude
  Code and Codex also their web search and web fetch.
- **Decisions of the operator (2026-11-27).** (1) A delegate's own web tools and own browser are blocked always, in every
  autonomy mode. (2) A delegate with a shell can still reach the network with ordinary commands (`curl`, `wget`, a script).
  Tool names cannot block that; it is named here as a residual for the later OS sandbox. Under "Standard" every shell call
  is already put to the person. (3) Computer use stays: a person must be able to ask a delegate to find, tidy or do
  something on their computer. HaexVault's isolation means HaexVault itself leaves as few traces on the system as
  possible; it does not keep a delegate away from the person's computer. (4) Abacus runs its web search and page fetch on
  its own server, where no rule of the program reaches them (measured, research R4). They stay, named as a residual;
  Abacus' own browser is blocked like the others'. (5) The external access is for programs outside HaexVault. A delegate
  HaexVault starts itself in the chat is no external access: it gets HaexVault's tools on every start, whether the
  external access is on or off.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - A delegate opens websites only through HaexVault's agent browser (Priority: P1)

A person asks a delegate (Claude Code, Codex or Abacus) to look something up on the web or open a website. The delegate
uses HaexVault's agent browser; its own web search, web fetch and browser are not available to it, so nothing it does on
the web bypasses HaexVault.

**Why this priority**: This is the purpose of the feature. It closes the gap between "the delegate is asked to use
HaexVault's browser" and "the delegate can only use HaexVault's browser".

**Independent Test**: For each delegate in "Standard" and in "Ungated", ask it to open a given website and to search the
web. Confirm the work is done through HaexVault's agent browser and that no own web tool or own browser of the delegate
runs (apart from the residual of FR-001).

**Acceptance Scenarios**:

1. **Given** a delegate in "Ungated", **When** the person asks it to open a website, **Then** the delegate opens it through
   HaexVault's agent browser and its own web fetch, web search and browser do not run.
2. **Given** the same in "Standard", **When** the delegate tries one of its own web tools anyway, **Then** the program
   refuses it without asking the person, and the person is never asked to approve the delegate's own web tool or own
   browser.
3. **Given** a delegate whose own web tool was refused, **When** it continues, **Then** the conversation goes on (the turn
   does not end because of the refusal).

---

### User Story 2 - A delegate has HaexVault's tools whether or not the external access is on (Priority: P1)

A person who never turned on the external access, because they do not connect any program from outside, asks a delegate
in the chat to open a website. The delegate has HaexVault's tools and the instruction, and what it may do with them is
decided by the agent profile of its model, as for every model. The external access still decides alone whether a program
outside HaexVault may connect.

**Why this priority**: The observed case. Without it the block of US1 leaves a delegate with no web at all, although the
person allowed the browser in the agent profile.

**Independent Test**: With the external access off, ask each delegate to open a website and confirm it uses HaexVault's
agent browser under its profile's rights. Then try to connect a program from outside (an MCP client) and confirm it is
refused.

**Acceptance Scenarios**:

1. **Given** the external access is off, **When** a delegate is asked to open a website, **Then** it opens it through
   HaexVault's agent browser, within the rights its model's agent profile gives.
2. **Given** the external access is off and a delegate is running, **When** a program outside HaexVault tries to connect
   to HaexVault, **Then** the connection is refused, as when no delegate runs.
3. **Given** the external access is turned on or off while delegates are used, **When** the next delegate starts, **Then**
   it has HaexVault's tools either way.
4. **Given** HaexVault's tools cannot be provided for a delegate start (the server cannot start), **When** the delegate
   answers, **Then** the chat says that HaexVault's tools were not available for this turn and why; the delegate's own web
   tools and browser stay blocked.

---

### User Story 3 - The person can see that the block holds (Priority: P2)

The block holds for every version of a delegate's program. The person or a maintainer can check, per delegate, which of
its own web tools and browser features are blocked, and a new program version cannot quietly bring a web tool or browser
past the block.

**Why this priority**: The block rests on switches of programs HaexVault does not control. A new version that adds a web
tool, or renames or drops a switch, must not reopen the gap unnoticed.

**Independent Test**: Read the recorded block per delegate; start a delegate whose program refuses the block switches and
confirm the request ends with a visible reason instead of running without the block.

**Acceptance Scenarios**:

1. **Given** any offered program version, **When** a delegate starts, **Then** it starts with the vendor's block.
2. **Given** a new program version with a web tool or browser feature the block does not name yet, **When** that version is
   checked before it is offered, **Then** the new tool is either covered by the block or the version is not offered.
3. **Given** a program that rejects a block switch or does not confirm it, **When** a delegate starts, **Then** the request
   ends with a reason the person can read and nothing runs without the block.

---

### Edge Cases

- A delegate's own skill or plugin grants itself its browser (Abacus' built-in browser skills allow their own browser
  command): the block still wins.
- The delegate reaches the same browser through another spelling (a wrapper command such as `npx`, a full path): the block
  covers the spellings the program's own rules can name; anything beyond that is shell network (residual, FR-008).
- A delegate uses computer use to open a browser on the person's desktop: computer use stays allowed (operator decision 3),
  so this is a residual like shell network (FR-008), not a breach of the block.
- The agent profile of the delegate's model does not give the browser: the delegate has no web (its own is blocked,
  HaexVault's refuses it), and the existing notice of a refused profile right (spec 083 US5) says why.
- A program outside HaexVault presents a delegate's pass: a pass is valid only for the start it was made for and ends with
  it (spec 021); outside that start it is refused like any unknown client.
- Model jobs (spec 055) run delegates in "Ungated" without HaexVault's server: they have no web, until a later spec gives
  jobs tools (recorded as planned). Decision 5 is about delegates in the chat.
- "Gated (permissive)" with a deny rule for network access: the block applies before any of the person's deny rules; the
  person's rules stay as they are for everything else.
- The block switches of one vendor change between versions: a version is only offered once its switches are confirmed
  (FR-006).
- A conversation that started before the change: no compatibility path; every new start of a delegate has the block and
  HaexVault's tools.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Every start of a subscription delegate in the chat (Claude Code, Codex, Abacus AI) MUST have the delegate's
  own web tools blocked: web search, web fetch and any tool of the program that loads a web page for the model. Excepted
  are web tools that run on the vendor's server, out of reach of the program's switches and rules (Abacus' web search and
  page fetch, operator decision 4); they are residuals under FR-008.
- **FR-002**: Every start MUST have the delegate's own browser features blocked, including browsers the program ships with
  or installs itself, browser commands its own skills or plugins allow themselves, and features that drive a browser of
  the person (such as "Claude in Chrome"). Computer use is not part of the block.
- **FR-003**: The block of FR-001 and FR-002 MUST hold in every autonomy mode ("Standard", "Ungated", "Gated (permissive)")
  and MUST NOT depend on the person's answer: a blocked tool is refused by the program, never put to the person as a
  question.
- **FR-004**: Every start of a delegate in the chat MUST give it HaexVault's tools through HaexVault's server, whether the
  device's external access is on or off. What the delegate may do with them stays decided by the agent profile of its
  model (spec 083).
- **FR-005**: The block MUST be enforced by the delegate's program through its own switches or deny rules, set by HaexVault
  on every start; it MUST NOT rest on the instruction to the model alone and MUST NOT rely on environment variables for its
  effect.
- **FR-006**: The block MUST hold for every offered program version. The block switches MUST be recorded per vendor, and a
  web tool or browser feature that a new program version brings MUST NOT be available to the delegate before it has been
  checked: when the program lets HaexVault name the tools that are allowed, new tools stay off by that; otherwise the
  block is checked for completeness when a version is added to spec 094's tested versions. A version whose program refuses
  a block switch, or whose new web tool is not covered, MUST NOT be offered.
- **FR-007**: If a delegate's program refuses a block switch at start or reports a state without the block, the request
  MUST end with a reason the person can read; nothing runs without the block and no other backend is tried.
- **FR-008**: Network access through a delegate's shell (`curl`, `wget`, a script), a browser opened through computer
  use, and web tools that run on the vendor's server (Abacus' web search and page fetch) are not covered by this feature.
  The spec and the Settings text that describes the block MUST name them as residuals; under "Standard" each shell call
  remains a question to the person.
- **FR-009**: The instruction to a delegate MUST stay: HaexVault's agent browser is the way to the web, and a failure is
  reported as what failed. With FR-004 every delegate in the chat gets it.
- **FR-010**: The block MUST NOT change what a delegate may do otherwise: its file, shell, edit and computer-use tools,
  HaexVault's tools and the person's approval posture stay as they are.
- **FR-011**: The external access MUST govern only programs outside HaexVault. While it is off, HaexVault MUST refuse every
  connection that is not a delegate HaexVault started for the current start, and MUST NOT open anything a program outside
  could reach without a delegate's pass. Its Settings text MUST say that it is for programs outside HaexVault and that
  delegates in the chat do not need it.
- **FR-012**: If HaexVault's tools cannot be provided for a delegate start, the chat MUST say so with the reason; the
  delegate's block still holds. The notice that asked the person to turn on the external access for a delegate
  (`chat.toolNotice.delegateOff`) and the status it rests on are removed (no compatibility path).

### Key Entities

- **Web block of a vendor**: for one delegate vendor, the switches and deny rules that turn off its own web tools and
  browser, how the program confirms them (if it does), and the program versions they were checked against.
- **Delegate pass**: the proof a delegate HaexVault started presents to HaexVault's server; it names the start and ends
  with it (spec 021). With FR-011 it is the only way in while the external access is off.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In a run per vendor (Claude Code, Codex, Abacus) and per mode ("Standard", "Ungated"), asked to open a website
  and to search the web, 0 of the delegate's own web tools or browser commands run, apart from the server-side residual
  of FR-001.
- **SC-002**: With the external access on and with it off, the same requests are answered through HaexVault's agent
  browser in every run where the delegate does them at all and its profile allows the browser.
- **SC-003**: With the external access off, a program outside HaexVault connects 0 times, also while delegates run, and 0
  browser profiles, sockets or downloads of a delegate's own browser appear outside the vault's folder.
- **SC-004**: The person is asked to approve a delegate's own web tool or own browser 0 times.
- **SC-005**: Every tested program version has a recorded web block, and a program that refuses a block switch ends the
  request with a readable reason in 100% of the starts.

## Assumptions

- The three programs in the vault's tested versions (Claude Code 2.1.311, Codex 0.169.0, Abacus AI 2.10.23) offer switches
  or deny rules for their own browser, and Claude Code and Codex for their web tools, that hold in their full-autonomy
  mode (measured, research R1–R4).
- A real run per vendor needs the operator's accounts and is an operator task; an Abacus run costs a few credits.
- No backward compatibility: there are no users yet, so no conversation or setting from before needs a migration.
- The delegate pass of spec 021 (made per start, living only in HaexVault's process) stays as it is; FR-011 only changes
  when the server runs and whom it accepts.
- HaexVault's agent browser itself (spec 021, 083), the built-in chat models and an OS sandbox are out of scope.
- HaexVault's own computer-use tools are out of scope; they would be a later spec of their own.
