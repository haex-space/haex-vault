# Contract: the web block per vendor

The arguments HaexVault adds to every start of a delegate in the chat and in model jobs, in every autonomy mode, and what
it checks before the first answer. Versions: Claude Code 2.1.311, Codex 0.169.0, Abacus AI 2.10.23 (research).

## Claude Code

**Arguments** (after the catalog's `config_args`):

```text
--tools Task,Bash,PowerShell,Read,Write,Edit,Glob,Grep,NotebookEdit,TodoWrite,TaskCreate,TaskUpdate,TaskGet,TaskList,
        TaskStop,Skill,REPL,JavaScript,AskUserQuestion,ToolSearch,SendUserMessage,CronCreate,CronDelete,CronList,
        DesignSync,EnterWorktree,ExitWorktree,ListAgents,Monitor,ReportFindings,ScheduleWakeup,SendMessage,Workflow
--no-chrome
```

(one comma-separated value; wrapped here for reading). `--allowedTools` for haex-vault's own tools stays as it is: it
decides asking, `--tools` decides availability, and MCP tools are not part of the built-in set.

**Confirmation**: the first `{"type":"system","subtype":"init"}` line.

| Field         | Must hold                                   | Otherwise                     |
| ------------- | ------------------------------------------- | ----------------------------- |
| `tools`       | contains neither `WebFetch` nor `WebSearch` | end the request, reason below |
| `mcp_servers` | no entry named `claude-in-chrome`           | end the request, reason below |

An init line without a `tools` field also ends the request (the block cannot be confirmed).

## Codex

**Arguments** (before `app-server`, after the catalog's `config_args`):

```text
-c web_search="disabled"
--disable browser_use --disable browser_use_external --disable browser_use_full_cdp_access
--disable browser_annotation_api --disable in_app_browser
```

**Confirmation**: after `initialize`/`initialized`, before `thread/start`:

| Request                    | Must hold                                                 |
| -------------------------- | --------------------------------------------------------- |
| `config/read`              | `config.web_search == "disabled"`                         |
| `experimentalFeature/list` | each of the five features is listed with `enabled: false` |

A missing feature, an error answer or another value ends the request. An unknown feature name in the arguments already
ends the program at start (`Unknown feature flag`); its stderr becomes the reason.

## Abacus AI

**Arguments** (in "Standard" and "Ungated"):

```text
--disallowed-tools web_search_text_only scrape_url_content WebSearch Fetch Bash(agent-browser:*) Bash(npx agent-browser:*)
```

Both name forms of the two web tools until research R4 measures which one the rules match; a `shell_start` rule follows
R4. **Confirmation**: none at start (the program reports no tool list); the version check stands in.

## Reason shown to the person

`AdapterError::Unavailable` with the text "{vendor} started without its web block ({what}); nothing was run." The chat
shows it like the other start failures of a delegate; no other backend is tried.
