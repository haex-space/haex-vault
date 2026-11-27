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
- **What was observed (2026-11-27).** The device's MCP access was off, so Abacus had no `haex` server and no instruction.
  Asked to open a website, it wanted to start its own bundled browser through its shell, with a browser profile under
  `/tmp/ab-profile`, that is outside the vault's folder. Under "Standard" the person was asked and could decline; under
  "Ungated" it would have run without a question. Claude Code and Codex have the same kind of own web tools; Codex also has
  its own browser features, and Claude Code can drive the person's Chrome ("Claude in Chrome").
- **Why it matters.** HaexVault's agent browser runs inside the vault's isolation, under the agent profile's rights
  (spec 083) and with its own approvals before buying, paying and sending. A delegate's own web tools and browser bypass all
  of that: they keep cookies and profiles where HaexVault does not look, act on websites without HaexVault's approvals, and
  bring web content into the conversation that spec 097 treats as foreign.
- **What the programs offer** (read from the vault's own versions on 2026-11-27, to be confirmed in research): each of the
  three programs can be started with its own web tools and browser switched off or denied by rule, and the programs state
  that an explicit deny holds also in their own full-autonomy mode.
- **Decisions of the operator (2026-11-27).** (1) A delegate's own web tools and own browser are blocked always: in every
  autonomy mode and also when the device's MCP access is off. Without the access the delegate has no web at all, and the
  existing notice explains how to turn the access on. (2) A delegate with a shell can still reach the network with
  ordinary commands (`curl`, `wget`, a script). Tool names cannot block that; it is named here as a residual for the later
  OS sandbox. Under "Standard" every shell call is already put to the person. (3) Computer use stays: a person must be
  able to ask a delegate to find, tidy or do something on their computer. HaexVault's isolation means HaexVault itself
  leaves as few traces on the system as possible; it does not keep a delegate away from the person's computer. (4) Abacus
  runs its web search and page fetch on its own server, where no rule of the program reaches them (measured, research
  R4). They stay, named as a residual; Abacus' own browser is blocked like the others' (2026-11-27).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - A delegate opens websites only through HaexVault's agent browser (Priority: P1)

A person with the MCP access on asks a delegate (Claude Code, Codex or Abacus) to look something up on the web or open a
website. The delegate uses HaexVault's agent browser; its own web search, web fetch and browser are not available to it, so
nothing it does on the web bypasses HaexVault.

**Why this priority**: This is the purpose of the feature. It closes the gap between "the delegate is asked to use
HaexVault's browser" and "the delegate can only use HaexVault's browser".

**Independent Test**: With the access on, for each delegate in "Standard" and in "Ungated", ask it to open a given website
and to search the web. Confirm the work is done through HaexVault's agent browser and that no own web tool or own browser
of the delegate runs.

**Acceptance Scenarios**:

1. **Given** the MCP access is on and a delegate in "Ungated", **When** the person asks it to open a website, **Then** the
   delegate opens it through HaexVault's agent browser and its own web fetch, web search and browser do not run.
2. **Given** the same in "Standard", **When** the delegate tries one of its own web tools anyway, **Then** the program
   refuses it without asking the person, and the person is never asked to approve the delegate's own web tool or own
   browser.
3. **Given** a delegate whose own web tool was refused, **When** it continues, **Then** the conversation goes on (the turn
   does not end because of the refusal).

---

### User Story 2 - Without the MCP access a delegate has no web and the person learns why (Priority: P1)

A person whose device has the MCP access off asks a delegate to open a website. The delegate cannot: its own web tools and
browser are blocked and HaexVault's are not available. The person sees the existing notice that the delegate reaches
HaexVault's tools only through the MCP access and where to turn it on.

**Why this priority**: The observed case. Without it the block would either fail silently or fall back to the delegate's
own tools.

**Independent Test**: Turn the access off, ask each delegate to open a website, confirm nothing of its own web tools or
browser runs, no file is written outside the vault's folder, and the notice is shown.

**Acceptance Scenarios**:

1. **Given** the MCP access is off, **When** a delegate is asked to open a website, **Then** none of its own web tools or
   browser runs and the chat shows the notice that names the setting to turn on.
2. **Given** the access is off and the delegate in "Ungated", **When** it tries its own browser, **Then** the program
   refuses it; nothing is started and no browser profile or socket is created anywhere.

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
- Model jobs (spec 055) run delegates in "Ungated" without HaexVault's server: they have no web either, until a later spec
  gives jobs tools (recorded as planned).
- "Gated (permissive)" with a deny rule for network access: the block applies before any of the person's deny rules; the
  person's rules stay as they are for everything else.
- The block switches of one vendor change between versions: a version is only offered once its switches are confirmed
  (FR-006).
- A conversation that started before the change: no compatibility path; every new start of a delegate has the block.

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
- **FR-004**: The block MUST hold whether or not the device's MCP access is on. When it is off, the chat MUST show the
  existing notice that the delegate reaches HaexVault's tools only through the MCP access and where to turn it on
  (`chat.toolNotice.delegateOff`), for every delegate vendor including Abacus.
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
  use, and web tools that run on the vendor's server (Abacus' web search and page fetch) are not covered by this feature. The spec, the delegate documentation and the Settings text that describe the block
  MUST name them as residuals; under "Standard" each shell call remains a question to the person.
- **FR-009**: The instruction to a delegate with HaexVault's server MUST stay: HaexVault's agent browser is the way to the
  web, and a failure is reported as what failed.
- **FR-010**: The block MUST NOT change what a delegate may do otherwise: its file, shell, edit and computer-use tools,
  HaexVault's tools and the person's approval posture stay as they are.

### Key Entities

- **Web block of a vendor**: for one delegate vendor and program version, the switches and deny rules that turn off its own
  web tools and browser, and how the program confirms them (if it does).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In a run per vendor (Claude Code, Codex, Abacus) and per mode ("Standard", "Ungated"), asked to open a website
  and to search the web, 0 of the delegate's own web tools or browser commands run, apart from the server-side residual
  of FR-001.
- **SC-002**: With the MCP access on, the same requests are answered through HaexVault's agent browser in every run where
  the delegate does them at all.
- **SC-003**: With the MCP access off, 0 browser profiles, sockets or downloads of a delegate's own browser appear outside
  the vault's folder, and the notice about the access is shown in 100% of the turns where the delegate's tools were
  needed.
- **SC-004**: The person is asked to approve a delegate's own web tool or own browser 0 times.
- **SC-005**: Every tested program version has a recorded web block, and a program that refuses a block switch ends the
  request with a readable reason in 100% of the starts.

## Assumptions

- The three programs in the vault's tested versions (Claude Code 2.1.311, Codex 0.169.0, Abacus AI 2.10.23) offer switches
  or deny rules for their own web tools and browser that hold in their full-autonomy mode; research confirms each with the
  program's own code or documentation and, where needed, a real run.
- A real run per vendor needs the operator's accounts and is an operator task; an Abacus run costs a few credits.
- No backward compatibility: there are no users yet, so no conversation or setting from before needs a migration.
- HaexVault's agent browser itself (spec 021, 083), the built-in chat models and an OS sandbox are out of scope.
- HaexVault's own computer-use tools are out of scope; they would be a later spec of their own.
- The notice for an absent MCP access exists (`chat.toolNotice.delegateOff`); this feature only makes sure it covers every
  delegate vendor.
