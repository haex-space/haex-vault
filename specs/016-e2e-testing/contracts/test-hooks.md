# Contract: the controls a scenario may reach

**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md)

FR-014: every control a scenario needs is reachable without depending on displayed text or the interface
language (the interface is German by default and its texts change). Use what already exists; add only
what is missing, as a `data-testid` attribute with a kebab-case name. An attribute changes neither
behavior nor appearance.

## Existing hooks

They are part of this contract now: renaming one means updating the helpers in the same change.

| Control                          | Hook                                               | Where                                       |
| -------------------------------- | -------------------------------------------------- | ------------------------------------------- |
| Unlock: passphrase field         | `#unlock-passphrase`                               | `src/components/onboarding/UnlockSheet.vue` |
| Unlock: submit button            | `[form="unlock-form"]`                             | same                                        |
| Create: name field               | `#create-name`                                     | `src/components/onboarding/CreateSheet.vue` |
| Create: passphrase, confirmation | `#create-passphrase`, `#create-passphrase-confirm` | same                                        |
| Create: submit button            | `[form="create-form"]`                             | same                                        |
| Closing page: the spinner        | `.ring`                                            | `public/closing.html`                       |
| Where the app is                 | `location.pathname` (`/workspace/…`, `/chat/…`)    | routes                                      |

## Hooks to add

| Hook                                                             | Element                                                               | File                                                     | Lines added |
| ---------------------------------------------------------------- | --------------------------------------------------------------------- | -------------------------------------------------------- | ----------- |
| `data-testid="instance-entry"` and `data-instance-name="<name>"` | The button of each instance in the start list                         | `src/components/onboarding/InstancesList.vue` (44 lines) | 2           |
| `data-testid="open-launcher"`                                    | The round button that opens the workspace app launcher                | `src/components/shell/ShellDesktop.vue`                  | 1           |
| `data-testid="open-chat"`                                        | The Chat tile in the workspace app launcher                           | `src/components/shell/ShellLauncher.vue`                 | 1           |
| `data-testid="lock-instance"`                                    | The lock tile in the app launcher (open it with `open-launcher`)      | `src/components/wm/Launcher.vue`                         | 1           |
| `data-testid="link-submit"`                                      | The submit button of the form "Mit einer Vault verknüpfen" (spec 033) | `src/components/onboarding/LinkSheet.vue`                | 1           |
| `data-testid="chat-input"`, `data-testid="chat-send"`            | The composer's text area and send button (spec 033)                   | `src/components/chat/Composer.vue`                       | 2           |
| `data-testid="chat-new"`, `data-testid="chat-thread"`            | The new-chat button and each row of the thread list (spec 033)        | `src/components/chat/ThreadSidebar.vue`                  | 2           |

`data-instance-name` carries the instance's name, which is data, not interface text, so an entry can be
found by name without reading what is displayed.

The lock control is the last tile of the launcher (it used to be a button in the chat sidebar and one in
the chat header, with a hook each; both are gone). It is reachable at every window size, so scenarios
open the launcher with `open-launcher` (the `openLauncher` flow) and press `lock-instance`. The chat's
sidebar is reached through `chat-sidebar-toggle`, which is also what `openChat` waits for.

Not added now: hooks for the message list or settings. Each scenario that needs one adds it with the
scenario, so no hook exists without a use.

## Finding a control

```ts
instance.click('open-launcher') // [data-testid="open-launcher"]
instance.click('open-chat') // [data-testid="open-chat"]
instance.click('#unlock-passphrase') // a selector starting with # . [ is used as is
```

A name without a selector character is a `data-testid`. If several elements match, the displayed one is
used; if none is displayed by the deadline, the call fails and names the hook.

## What the closing page must look like

Not a hook but a contract the closing-page scenario relies on: no text at all, exactly one `.ring`, and a
body background equal to the page's own `--page` colour for the colour scheme in force
(`public/closing.html`, spec 013).
