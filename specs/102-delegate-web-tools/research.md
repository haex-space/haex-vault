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

## R3. Abacus AI: deny rules in the settings file, in both modes

**Decision**: every Abacus start passes a settings file whose `permissions.deny` holds `Bash(agent-browser *)`,
`Bash(agent-browser)` and `Bash(npx agent-browser *)`: in "Standard" next to the ask rule `*` in the file it already gets,
in "Ungated" as a file with only these deny rules. Its web search and page fetch cannot be blocked from the program (R4);
the operator named them a residual (spec decision 4, FR-001, FR-008).

**Rationale**:

- Read from the program: the flag fills the permission store's `cliDenied` rules, which join the settings file's deny rules
  in `denyRules`; deny is evaluated before ask and allow, and before the YOLO auto-allow (spec 096 research §15). A skill's
  own `allowed-tools` (the built-in `browser-use` skill allows `Bash(agent-browser:*)`) land in `skillAllowRules`, an allow
  source, so a deny still wins.
- Read from the program: a call's rule key is `Bash` with the command as its subject for the shell tool, and the internal
  tool name for every other tool (`web_search_text_only` for "WebSearch", `scrape_url_content` for "Fetch"). A compound
  command is judged per segment and a denied segment denies the whole command, so `export X=...; agent-browser open ...`
  is caught.
- Abacus' init line carries no tool list and `get_settings` reports only configured rules, so there is no confirmation at
  start; the version check (R5) and the measured run (R4) stand in for it.

**Alternatives considered**: `--disallowed-tools` on the command line. Its help says "comma- or space-separated", so
`Bash(npx agent-browser *)` would be cut at its space; the settings file takes each rule whole.

## R4. Abacus: what its rules can block (measured 2026-11-27)

Three real runs with the operator's sign-in (a few credits), each asking Abacus to search the web, fetch a page, run
`agent-browser --version`, `npx agent-browser --version` and `export X=1; agent-browser --version`, and start a background
shell (`shell_start`).

| Run                                                                          | Web search, page fetch | Browser commands                                    |
| ---------------------------------------------------------------------------- | ---------------------- | --------------------------------------------------- |
| "Standard", ask `*`, `--disallowed-tools` with internal names and `:*` rules | ran, no question       | asked (rule did not match)                          |
| "Standard", settings: ask `*`, deny with both name forms and `*` rules       | ran, no question       | refused by the rule, no question                    |
| "Ungated", settings: deny `*` rules only                                     | ran                    | refused by the rule ("denied by a permission rule") |

**Findings**:

- The rule syntax is `Bash(<command> *)`, as in the program's own description (`Bash(git log *)`), not `:*`. With it the
  bundled browser is refused in both modes, also through `npx` and behind `export X=1;`.
- Web search (`web_search_text_only`) and page fetch (`scrape_url_content`) run in Abacus' server loop: they never reached
  the program's permission check (no question even under ask `*`), and no deny form stopped them. No setting turns them
  off (all 47 setting definitions read through `list_setting_definitions`). They leave nothing on the computer, cannot sign
  in or act on a website, and bring the result into the conversation as foreign content.
- `shell_start` is not in the print mode's tool set; the model said so in each run. Background shells are no path here.

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

## R6. HaexVault's server for delegates without the external access (spec FR-004, FR-011, FR-012)

**Today** (read 2026-11-27): `McpServer` in `mcp_server/mod.rs` starts `server::start` only while the device's
`mcp.enabled` is on, with the local endpoint (`program_way`, a Unix socket in the user's runtime folder or a named pipe)
and, where the platform has it, the HTTP endpoint on `127.0.0.1`. `server::start` attaches the core to the `DelegateHub`;
without a running server `DelegateHub::server_for` answers `None` ("the server is not running"). A delegate connects
through the helper on the local endpoint with `--delegate-token`; `hello.rs` marks such a connection `delegate: true`.

**Decision**: the server runs while a vault is open, whatever the external access says. The external access decides two
things only: whether the HTTP endpoint is open, and whether the local endpoint accepts anything but a delegate's pass.
While it is off, a connection whose hello is not a delegate with a known, running pass is closed before any tool is
listed (spec FR-011). `delegateOff`, its i18n texts, `mcp_delegate_status` and `DelegateStatus` go. `server_for` today
returns `None` silently (only a log line) on five paths: no running server, the model not offered by the active profile,
a write error, no helper path, no pass (`mcp_server/delegates.rs`). It returns the reason instead, and the driver emits
it to the chat for that start through the context's `EventEmitter` (FR-012); a job start only logs. On Android and iOS,
which have no delegates, the server keeps starting only with the access on.

**Rationale**: the pass already exists, lives only in the process and ends with the start (spec 021), so it is a sufficient
gate on the local endpoint. HTTP is the way programs like an editor connect; keeping it closed while the access is off
leaves nothing reachable without a pass. The socket lies where it lies today with the access on (the runtime folder, a
`tmpfs` on Linux, removed when the server stops).

**Alternatives considered**:

- Start the server only for the time a delegate runs. Fewer minutes with a socket, but a start and stop per turn, and
  parallel turns (ADR 0021) would share and race over one server; rejected.
- A second, delegate-only server in the run folder inside the vault. No socket outside the vault, but a second copy of
  the server's state and Unix socket paths are limited to about 100 bytes, which a vault path can exceed; rejected for
  now, noted for the OS sandbox spec.

## R7. Residuals (spec FR-008)

- Shell network: `curl`, `wget`, a script in the delegate's shell. Under "Standard" every shell call is put to the person.
- Computer use: Codex' `computer_use` stays on, and a delegate can open a desktop browser through it.
- Abacus: its web search and page fetch, which run on its server (R4).

They are named in the Settings text about the autonomy modes of the delegates.
