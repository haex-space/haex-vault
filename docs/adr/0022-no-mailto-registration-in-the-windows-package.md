# holzi does not register `mailto` in the Windows package

Status: proposed
Date: 2026-10-29

## Context

Spec 063 lets holzi open `mailto:` links from other apps, so a person can pick it, and hand them to an
extension. FR-016 says holzi never makes itself the default program for such links and never changes a
system setting; the choice stays with the person.

The Tauri bundler registers a scheme from `plugins.deep-link.desktop.schemes` the same way for every
scheme. The NSIS template writes `Software\Classes\<scheme>` (`URL Protocol`, `DefaultIcon`,
`shell\open\command`) and the WiX template writes the same keys under `HKLM`. For `mailto` that key is the
class of the mail program of the machine. Writing it replaces that class and makes holzi the program for
every mail link, and the uninstaller deletes the key again, which leaves the machine without one. Nothing
in the installers lists holzi as a program to choose instead (`RegisteredApplications`). The Linux package
only lists `x-scheme-handler/mailto` in `MimeType` of the `.desktop` file, and macOS lists the scheme in
`CFBundleURLSchemes`; both leave the choice to the person.

## Decision

**`mailto` is a scheme of the Linux, macOS and Android packages and not of the Windows package.**

- `tauri.conf.json` lists `haex-vault` and `mailto` for the desktop and for Android.
  `tauri.windows.conf.json` overrides `plugins.deep-link.desktop.schemes` with `["haex-vault"]` (a config
  file of a platform is merged over the base file).
- `deep_link::register_for_development` registers `haex-vault` only. `register_all` would also run
  `xdg-mime default` for `mailto` on a developer's Linux machine.
- On Windows a mail link from another app does not reach holzi. The mail button and the choice in the
  address book work there, since they do not depend on the registration (spec 063 FR-016 and SC-005 name
  this exception).
- `scripts/check-extensions-openers-registration.ts` fails when the Windows file lists another scheme than
  `haex-vault`, or when `register_for_development` calls `register_all`.

## Consequences

- A Windows user cannot pick holzi for mail links. If that is wanted later, it needs a registration that
  only lists holzi as a choice (`RegisteredApplications` and a `Capabilities` key, written by an installer
  hook), not the class key of the scheme; that is a separate decision.
- Any further scheme added to `plugins.deep-link.desktop.schemes` for a system-wide scheme (`tel:`, `sms:`,
  `geo:`) has the same problem on Windows and needs the same check before it ships.
- The decision rests on the bundler templates of the CLI in use (2.6.2, read in its sources, not tried in a
  Windows build); a newer bundler may register differently and the reasoning has to be read again then.

## Alternatives considered

- **Keep `mailto` in the Windows package.** Simple, and the link would arrive, but it replaces the mail
  program of the machine, which FR-016 forbids.
- **Write the registration in an installer hook that only lists holzi as selectable.** Right in effect, but
  an own piece of installer code for one scheme, untested here and beyond this spec.
