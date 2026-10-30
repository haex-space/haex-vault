# Agents drive a browser that holzi starts itself, over a pipe, from an isolated world

Status: accepted
Date: 2026-10-28; accepted 2026-10-30 with spec 065 PR C3

## Context

Spec 065 lets agents (the built-in one, external agents over the MCP access of ADR-0005, later the
delegates) operate a web browser. A browser an agent can steer is a key to everything the user can
do on the web, and every page it reads may try to steer the agent (prompt injection). The spec
requires that only holzi can steer the browser (FR-004), that the user sees and can stop every
session, that credentials never reach a model, and that the browser profile is encrypted on this
device only (clarification 2026-10-28). A browser tab inside holzi's own webview is out of scope.

## Decision

**holzi starts its own Chromium-based browser and steers it over `--remote-debugging-pipe`.** No
debugging port is ever opened: a pipe reaches only the parent process. On Windows the pipe uses the
CRT handle inheritance that Puppeteer uses. holzi talks CDP through a small client of its own, not
through `chromiumoxide` or `headless_chrome`, which require a port.

**Everything holzi runs in a page runs in an isolated world.** Reading, element numbering, the
marker and its buttons live in `Page.createIsolatedWorld`; the buttons report over a binding that
exists only in that world, and untrusted events are ignored. The page shares the DOM but cannot
call or replace holzi's code. Clicks and typing are real input events. Chromium installs such a
binding in later documents only while the `Runtime` domain is enabled on the tab, so holzi enables
it there (seen in the PR C3 test against a real Chromium).

**The rules run in Rust, not in the page.** Which websites an agent may open is checked before a
document loads; what counts as submitting is decided before the click and, as a safety net, by
holding the page's changing requests right after an agent action. A page cannot skip a prompt,
allow a website or change a right.

**Between runs the profile lives in an encrypted folder.** When the browser ends, holzi packs the
lasting parts of its profile into `profile.zip` in an encrypted folder of spec 048 in holzi's data
directory on this device; there is no second encryption format and no key of its own. While the
browser runs, the profile is in an owner-only working directory that holzi deletes at the end of
every run and before every start. The browser saves no passwords: credentials come only from the
password manager, and new ones are stored there (spec FR-002, FR-036).

**Browser tools are native catalog actions (ADR 0011) with `confirmsItself`.** One definition serves
the built-in agent and the MCP access; the browser service asks the user itself, once per action.

## Consequences

- No other program on the device can attach to the agent browser.
- After a crash of holzi, the working directory stays readable to the same user until the next
  start deletes it; a stolen device is covered by the system's disk encryption, not by holzi.
- The profile file can be read with the vault's content key; it never leaves this device's data
  directory.
- holzi maintains a small CDP client (about 35 methods); Chromium changes CDP rarely for these, and
  a minimum version (120) is checked at start.
- Flatpak or Snap browsers whose sandbox breaks the pipe are not supported.
- Prompt injection is contained by what an agent can do, not by detecting it; a fixed corpus of
  injection pages checks the rules.
- Firefox is not supported as agent browser.
