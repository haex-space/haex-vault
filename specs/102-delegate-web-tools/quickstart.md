# Quickstart: checking the web block with the real programs

Run by the operator with their own accounts, in a vault with the delegates installed. Abacus runs cost a few credits.

## 1. MCP access on, per delegate, "Standard" and "Ungated"

1. Settings → AI → External access: on.
2. In a new chat with the delegate: "Open https://example.org and tell me the heading." Then: "Search the web for the
   current Rust release."
3. Expect: the work goes through HaexVault's agent browser (its calls appear as HaexVault tool calls); the delegate's own
   web search, web fetch or browser is never asked about and never runs (SC-001, SC-002, SC-004).

## 2. MCP access off

1. Settings → AI → External access: off.
2. Same requests as §1 in "Ungated".
3. Expect: the notice "{name} reaches haex tools only through the MCP access …" appears once; the delegate says it cannot
   open the page; no folder like `/tmp/ab-profile`, no browser socket and no download appear outside the vault
   (`find /tmp ~ -newer <marker> -maxdepth 2` before and after) (SC-003).

## 3. Abacus' browser (research R4)

In "Standard" and "Ungated", ask Abacus to run `agent-browser --version`, `npx agent-browser --version` and
`export X=1; agent-browser --version`. Expect each to be refused with "Tool was denied by a permission rule", without a
question. Its web search and page fetch still run (server-side residual).

## 4. A new program version

Before a version is added to `catalog.json`:

- Claude Code: run it once without credentials (`-p hi --output-format stream-json --verbose` in a throwaway home) and
  compare the init line's `tools` with the allow-list; add a new non-web tool to the allow-list, leave a new web tool out.
- Codex: `codex features list` in a throwaway `CODEX_HOME`; any new feature that loads web pages or drives a browser gets
  a `--disable`.
- Abacus: check that the bundled browser is still called `agent-browser` (its built-in skills' `allowed-tools`) and that
  no new tool reaches the web from the computer; repeat §3.
- Then add the version to the vendor's `checked_versions` in `web_block.rs`; the catalog test fails without it.
