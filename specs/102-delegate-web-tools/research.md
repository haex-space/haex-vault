# Research: Delegates reach the web only through HaexVault's agent browser

Every finding below was read from or measured with the vault's own program versions (Claude Code 2.1.311, Codex 0.169.0,
Abacus AI 2.10.23) on 2026-11-27. The measurements ran each program in a throwaway home without credentials, so no account
was used and no model was called; the Abacus rule-name run (R4) is the exception and is still open.

## R1. Claude Code: allow-list of built-in tools, not a deny-list

**Decision**: start every Claude Code delegate with `--tools <list>`, the built-in tools it may use, which leaves out
`WebFetch` and `WebSearch`, and with `--no-chrome`. At start, read the `tools` and `mcp_servers` fields of the `system/init`
line and end the request if `WebFetch` or `WebSearch` is listed or a `claude-in-chrome` server is connected.

**Rationale**:

- Measured: `--tools Task,Bash,Edit,Read,Write` makes the init line report exactly those tools, plus the tools of the MCP
  servers (`mcp__haex__browser_open` with a stand-in `haex` server). A tool a later version adds is not in the list and stays
  off, which is what spec FR-006 asks for when the program can name the allowed tools.
- Measured: an unknown name in `--tools` (and in `--disallowedTools`) is dropped without an error, so the program does not
  tell HaexVault that the block failed. The init line does, which is why it is read (spec FR-007).
- Measured: the default set of 2.1.311 in the init line is `Task, Bash, CronCreate, CronDelete, CronList, DesignSync, Edit,
EnterWorktree, ExitWorktree, ListAgents, Monitor, NotebookEdit, Read, ReportFindings, ScheduleWakeup, SendMessage,
Skill, TaskStop, ToolSearch, WebFetch, WebSearch, Workflow, Write`. The program's own list of built-in names
  (`BUILTIN_TOOL_NAMES`) adds `Glob, Grep, TodoWrite, TaskCreate, TaskUpdate, TaskGet, TaskList, REPL, JavaScript,
AskUserQuestion, SendUserMessage`; on Windows `PowerShell` takes the place of or joins `Bash`. The allow-list is the union
  of both without the two web tools, so nothing a delegate has today goes away (spec FR-010); names absent on a platform
  are dropped by the program.
- "Claude in Chrome" drives the person's own Chrome through an MCP server the program adds itself (`--chrome` /
  `--no-chrome`, needs a claude.ai subscription and the extension). It is not a built-in tool, so `--tools` does not cover
  it; `--no-chrome` does, and the init line's `mcp_servers` shows it.

**Alternatives considered**:

- `--disallowedTools WebFetch WebSearch`: measured to remove both from the init line, also under `bypassPermissions`. It
  was the first choice, but a web tool a later version adds would stay on until someone notices; the allow-list closes
  that without a check per version.
- `--restricted`: removes the tools that run commands; that changes far more than the web (spec FR-010).

## R2. Codex: features off at start, confirmed through the app-server

**Decision**: start every Codex delegate with `-c web_search="disabled"` and `--disable` for `browser_use`,
`browser_use_external`, `browser_use_full_cdp_access`, `browser_annotation_api` and `in_app_browser`. `computer_use` stays
on (operator decision 3). After `initialize`, call `config/read` and `experimentalFeature/list` and end the request if
`web_search` is not `disabled` or one of the five features is enabled.

**Rationale**:

- Measured with `codex features list`: all five features are `stable` and on by default; after the switches they are off,
  `computer_use` stays on.
- Measured: `--disable no_such_feature` ends the program with `Error: Unknown feature flag: no_such_feature`, and
  `-c web_search="nonsense"` with `failed to load bootstrap configuration`. A renamed switch therefore fails the start
  loudly, and the request ends with the program's reason (spec FR-007).
- Measured: the app-server answers `config/read` with `web_search: "disabled"` and the features, and
  `experimentalFeature/list` with `enabled` per feature, before any thread starts and without a login.
- Codex has no allow-list of its tools. A feature a later version adds is caught by the version check of R5.
- The switches go where `config_args` already go (before the subcommand), next to the catalog's `--disable plugins`.

**Alternatives considered**: putting the switches into `catalog.json`'s `config_args`. Rejected: those also reach the
sign-in and install runs, and the confirmation needs the same list in code; one list in code serves both start and check.

## R3. Abacus AI: deny rules on the command line, in both modes

**Decision**: start every Abacus delegate with `--disallowed-tools` naming its web search, its page fetch, and its bundled
browser command in the spellings its rules can match (`Bash(agent-browser:*)`, `Bash(npx agent-browser:*)`), in
"Standard" and in "Ungated". The exact names are those confirmed by R4.

**Rationale**:

- Read from the program: the flag fills the permission store's `cliDenied` rules, which join the settings file's deny rules
  in `denyRules`; deny is evaluated before ask and allow, and before the YOLO auto-allow (spec 096 research §15). A skill's
  own `allowed-tools` (the built-in `browser-use` skill allows `Bash(agent-browser:*)`) land in `skillAllowRules`, an allow
  source, so a deny still wins.
- Read from the program: a call's rule key is `Bash` with the command as its subject for the shell tool, and the internal
  tool name for every other tool (`web_search_text_only` for "WebSearch", `scrape_url_content` for "Fetch"). A compound
  command is judged per segment and a denied segment denies the whole command, so `export X=...; agent-browser open ...`
  is caught.
- The command-line flag works without a settings file, which "Ungated" does not pass today (spec 096 #1176).
- Abacus' init line carries no tool list and `get_settings` reports only configured rules, so there is no confirmation at
  start; the version check (R5) and the measured run (R4) stand in for it.

**Alternatives considered**: deny rules in the `--settings` file. They work the same, but "Ungated" would need a settings
file only for them, and the file is written to the run folder; the flag is simpler.

## R4. Abacus rule names and background shells (open, needs a real run)

**Open questions**:

1. Does a deny rule match the internal name (`web_search_text_only`, `scrape_url_content`), the display name
   (`WebSearch`, `Fetch`), or both? The plan passes both until the run decides.
2. Does `shell_start` (a background shell) go through the same `Bash` rule check? Its rule key in the code read is the tool
   name, not `Bash`. If it is not covered, `shell_start` needs its own deny rule with the browser command, or, if its rules
   cannot name a command, the residual of spec FR-008 grows by it and the spec says so.
3. Does `--disallowed-tools` take several rules after one flag, or one rule per flag? The start without a model call
   (a catalog request, as in spec 096) can show a parse error without credits; the run settles the rest.

**How**: one real run per mode, asking Abacus to search the web, fetch a page, open a page with its browser, and start
the browser in a background shell; record which calls were refused by the program without a question. It uses the
operator's sign-in and costs a few credits.

## R5. Recording the block per vendor and checking new versions

**Decision**: one module holds, per vendor, the switches and the names the confirmation looks for, and the program
versions they were checked against. A unit test over the shipped catalog fails when a vendor's tested versions include
one that the web block was not checked against. Adding a version to the catalog therefore needs the check of
[quickstart.md](quickstart.md) and an entry in the module in the same change.

**Rationale**: the catalog is the only way a version reaches a vault (spec 094 FR-009), so a test over it stops an
unchecked version before release. The block itself is the same for every version (operator, 2026-11-27); only the record
of which versions were checked grows.

**Alternatives considered**: a check at the start of each delegate against the full list of the program's tools or
features. Rejected: it would refuse a vault's installed version over a tool that has nothing to do with the web, and a
version only arrives through the catalog anyway.

## R6. The notice without the MCP access

**Finding**: the notice already covers Abacus. A delegate turn emits `chat-tool-availability` with the delegate's kind
(`chat/send.rs`), the frontend maps `abacusDelegate` to "Abacus AI" (`useToolNotice.ts`), and `mcp_delegate_status` answers
`off` when the access is off. Spec FR-004 needs a test that proves it for Abacus, no new code.

## R7. Residuals (spec FR-008)

- Shell network: `curl`, `wget`, a script in the delegate's shell. Under "Standard" every shell call is put to the person.
- Computer use: Codex' `computer_use` stays on, and a delegate can open a desktop browser through it.
- Abacus: anything its rules cannot name (R4).

They are named in the delegate documentation and in the Settings text about the delegates.
