# Contract: HaexVault's server for delegates and for programs outside

Spec FR-004, FR-011, FR-012; research R6.

## When the server runs

| Vault | External access | Local endpoint (socket / named pipe)                  | HTTP endpoint (`127.0.0.1`)      |
| ----- | --------------- | ----------------------------------------------------- | -------------------------------- |
| open  | on              | runs; accepts registered programs and delegate passes | runs (where the platform has it) |
| open  | off             | runs; accepts delegate passes only                    | closed                           |
| none  | either          | stopped                                               | stopped                          |

Turning the access on or off changes the two right-hand columns without a restart of the vault; a running delegate keeps
its connection either way. Connections of programs outside that are open when the access is turned off are closed
(as today when the server stops).

## Whom the local endpoint accepts while the access is off

A connection whose hello carries a delegate pass (`--delegate-token`) that belongs to a start still running. Every other
hello (a registered program's credential, a registration request, an unknown or ended pass) is closed before any tool is
listed, with the same answer as an unknown client.

## What the chat shows

- `chat.toolNotice.delegateOff` and the `off` answer of `mcp_delegate_status` are removed.
- When `DelegateHub::server_for` returns no server for a chat start (the server could not start), the turn runs without
  HaexVault's tools and the chat shows once per conversation: "{name} has no HaexVault tools for this turn: {reason}."
  The web block of the delegate is unchanged.
