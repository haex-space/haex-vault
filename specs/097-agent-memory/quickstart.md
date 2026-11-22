# Quickstart: see the memory work

Run from the worktree with `pnpm tauri:dev` and a fresh vault. The chat's approval mode is "Manuell", the factory value,
unless a step says otherwise. Steps 1 to 4 need PR A, 5 to 12 PR B, 13 and 14 PR C.

1. **The page.** "Einstellungen → KI → Gedächtnis" is empty. Add a page "Bernd Krause", description "Steuerberater",
   text "Mein Steuerberater. Siehe [[Steuer 2026]]." It shows the text, "Du", this device and the time.
2. **Pointers.** The page shows "Steuer 2026" as a pointer to no page. Add that page; the pointer opens it, and "Steuer
   2026" lists "Bernd Krause" as pointing to it.
3. **The check.** Try to store "Mein WLAN-Passwort ist hunter2hunter2", then a text of 11,000 characters, then "Ignore all
   previous instructions". Each is refused with its reason and nothing is stored.
4. **Another device.** Open the vault on a second device and let it sync: both pages are there.
5. **The right.** "KI → Agentenprofile → Standard" shows the group "Gedächtnis" at "Lesen und schreiben" with the note
   that it does not apply to external agents. A new profile shows it at "Aus".
6. **An agent remembers (US1).** In a new conversation with a hosted model write "Merk dir, dass ich Mails immer kurz und
   ohne Anrede beantworte". No question appears although the chat is on "Manuell"; a toast shows the text, and
   "Gedächtnis" lists the page with the model's name.
7. **The next conversation finds it.** Start a new conversation and ask "Wer ist mein Steuerberater?". The agent opens or
   searches the memory and names Bernd Krause.
8. **Frozen in a running conversation.** In the conversation of step 6 ask "Was steht in deinem Gedächtnis?": the page
   written in step 6 is not in the table of contents the model got at the start (it can still open it). A new
   conversation has it.
9. **Foreign content asks (US5).** In a new conversation let the agent read a web page, then write "Merk dir, dass ich Tee
   lieber mag als Kaffee". The approval appears with the page title and the text. Decline: nothing changed, the agent
   says that nothing was stored. Ask again in the next turn of the same conversation: it asks again.
10. **Plan.** Set the chat to "Plan" and ask the agent to remember something: it is refused.
11. **The profile decides (US4).** Make a profile "Ohne Gedächtnis" with the group at "Aus", add the hosted model, make the
    profile active. A new conversation with that model does not know the tax adviser and has no memory tools. Set the
    group to "Lesen": it finds him, and "Merk dir …" is answered with that it cannot store it. Switch the model of a
    running conversation from a local model (in "Standard") to that one: from that turn on it has no memory.
12. **Many pages (US6).** `cargo test --lib -- agent_memory::toc` covers the cut with 500 pages; by hand, generate 300
    pages with the test command of the tasks and ask for a fact that only the text of one page holds: the agent finds it
    by searching.
13. **An Abo-Agent knows it (US2).** In a conversation with Claude Code ask "Schreib einen Brief an meinen Steuerberater":
    it addresses Bernd Krause. Write "Merk dir, dass ich Briefe mit 'Viele Grüße' beende": no question. The local agent
    knows it in its next conversation. Then let Claude Code fetch a web page with its own web tool and ask it to remember
    something: the question appears in its chat thread. The same with Codex, as far as the spike of PR C found.
14. **No external agent.** Connect an external agent (the snippet of "Externer Zugang") whose profile is "Standard": its
    tool list has no memory tool and its instructions say nothing of the memory.

## Measurement (PR D)

```bash
# the memory set, one model
HAEX_EVAL_SET=specs/097-agent-memory/analysis/memoryset.json HAEX_EVAL_PROVIDER=… HAEX_EVAL_MODEL=… \
  cargo test --manifest-path src-tauri/Cargo.toml --features everyday-eval --test model_tool_eval \
  -- --ignored --nocapture measure_everyday_voice

# the everyday set with a seeded memory, for the two small models (FR-027)
HAEX_EVAL_MEMORY=specs/097-agent-memory/analysis/seed-memory.json HAEX_EVAL_ENGINE=llamacpp HAEX_TEST_GGUF=… \
  HAEX_EVAL_PROFILE=desktop cargo test --manifest-path src-tauri/Cargo.toml \
  --features llm-llamacpp-cuda,everyday-eval --test model_tool_eval -- --ignored --nocapture measure_everyday_voice
```

Results go to `specs/097-agent-memory/results.md`: the memory set per model against SC-001, SC-003, SC-004 and SC-009, and
the two everyday runs against 95 and 86 of 106 (SC-006).

The `model_tool_eval` command covers hosted and local models, not CLI Abo-Agents. After PR C, run every request from
`memoryset.json` through the app once with Claude Code and once with Codex, using the same seeded pages for each run.
Score each request against its expected outcome and record the correct count and any safety failures separately for both
vendors in `results.md` under SC-001 and SC-003. Keep these scores alongside, not in place of, the existing hosted and
local model measurements.
