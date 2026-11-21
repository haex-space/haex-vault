# Contract: events

| Event                            | Payload               | Sent when                                                                                                                                                       | Listener                                        |
| -------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| `haex:diagnostic-log-changed`    | `DiagnosticLogStatus` | at the end of **every** `begin` (the unlock, after a reset of an ended level or not), and whenever the level changes by `diagnostic_log_set` or by the end time | the settings component and the interface plugin |
| `haex:diagnostic-log-unwritable` | `{}`                  | the first failed write after a successful one (not on every line)                                                                                               | the settings component                          |

The interface plugin loads at the start of the app, before any vault is unlocked, so it cannot ask for the level then. It stays at
"Standard" (it forwards only warnings and errors, which the backend would let through at any level) until the event of `begin`
arrives, and keeps the level from every later event. When the window is reloaded while a vault is open, the plugin also calls
`diagnostic_log_get` once at its start; an error "no active vault" there means no unlock yet and changes nothing.
