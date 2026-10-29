# Agents drive a browser that holzi starts itself, over a pipe, from an isolated world

Status: proposed
Date: 2026-10-28

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
call or replace holzi's code. Clicks and typing are real input events.

**The rules run in Rust, not in the page.** Which websites an agent may open is checked before a
document loads; what counts as submitting is decided before the click and, as a safety net, by
holding the page's changing requests right after an agent action. A page cannot skip a prompt,
allow a website or change a right.

**The profile is encrypted during and between runs.** While the browser runs, its profile is in a
per-run encrypted ephemeral container whose key exists only in process memory. When it ends, holzi
packs the lasting parts into an XChaCha20-Poly1305 STREAM archive under a random key stored in the
operating system's protected credential store, bound to this app installation and never copied with
the vault. holzi uses the archive key only while the vault is unlocked, then destroys the working
container.

**Browser tools are native catalog actions (ADR 0011) with `confirmsItself`.** One definition serves
the built-in agent and the MCP access; the browser service asks the user itself, once per action.

## Consequences

- No other program on the device can attach to the agent browser.
- If the operating system cannot provide an encrypted ephemeral profile container, holzi disables
  the agent browser on that device rather than leave profile data readable after a crash.
- holzi maintains a small CDP client (about 35 methods); Chromium changes CDP rarely for these, and
  a minimum version (120) is checked at start.
- Flatpak or Snap browsers whose sandbox breaks the pipe are not supported.
- Prompt injection is contained by what an agent can do, not by detecting it; a fixed corpus of
  injection pages checks the rules.
- Firefox is not supported as agent browser.
