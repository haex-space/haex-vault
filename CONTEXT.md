# holzi

Portable personal-agent Tauri app. One user, one operator, small set of
their own devices sharing an encrypted SQLite vault via CRDT sync.

## Language

### Identity model

**Vault**:
The encrypted SQLite database file (`*.db`) plus its schema. The unit
of ownership: one vault holds one operator's data.
_Avoid_: instance (used to mean this earlier, deprecated), database.

**Installation UUID**:
Per-app-installation identifier stored in a file at
`<AppLocalData>/installation-id`. The corresponding `installation_uuid` is
also recorded in the sync-tracked `known_devices` table, so a copied vault
can carry that historical row with it. The lookup key `HolziBootstrap` uses to
decide whether the current open is genesis, resume, or adoption of a copied
vault; adoption mints a fresh `vault_device_uuid` for the new installation.
_Avoid_: device id (ambiguous — see below).

**Vault Device UUID**:
Per-`(vault × installation)` identifier stored in `known_devices`. The
CRDT `device_id` for HLC timestamps and signatures on this replica.
Different on every device that has ever opened this vault, including a
device that opened a copy of the vault (the adoption path mints a new
one). Used as the FK target for per-device rows.
_Avoid_: device_id (unqualified — usually means this, but callers
sometimes mean installation_uuid).

**Vault Identity Keypair**:
Singleton `vault_identity` row containing the vault's secp256k1
keypair. Copies of the vault share the same keypair — this is the
identity the vault itself holds and moves with the file.

**Adoption**:
The `HolziBootstrap` path taken when a copied vault file is opened on
a device that has never seen it. Mints a fresh `vault_device_uuid`,
which automatically isolates the new device from the source device's
per-device rows.

### Scope conventions

**Per-vault data**:
Rows that apply to the whole vault across every device. Examples:
`providers`, `models`, `chat_threads`, `chat_messages`,
`vault_preferences` entries (see below). Sync-tracked, no
device-discriminator column.

**Per-device data**:
Rows that apply only to a specific device. Composite PK includes
`vault_device_uuid` with FK to `known_devices`. See ADR-0001.

**Vault Scope Sentinel**:
The nil UUID `00000000-0000-0000-0000-000000000000` — reserved as a
row in `known_devices` representing "vault-wide" in tables that would
otherwise be device-scoped. See ADR-0001.

### Model catalog

**Catalog entry**:
One of the compiled-in GGUF suggestions under
`src-tauri/src/catalog/model_catalog.json`. Not installed until the
user downloads it.

**Installed model**:
A GGUF file physically present at
`<AppLocalData>/models/<slug>/<filename>` on the current device. The
filesystem is the authority for "installed on this device"; the
`models` table only carries catalog metadata (name, context window,
tokenizer repo, provider id).

**Provider**:
An entry in `providers`: `local` (mistralrs in-process), `api_key`
(external HTTP API such as Anthropic), or `cli_delegate` (spawn an
external CLI — design pass open).

**Model ID**:
Cross-provider unique string. For local models: catalog id. For api_key
models: `<provider_uuid>:<remote_model_id>` composite. Persisted verbatim
as `chat_messages.model_id`.

**Fit**:
Hardware verdict for a catalog entry — `Fits` / `Tight` / `TooBig` /
`Unknown`. See `hardware::fit`.

### Chat runtime

**Adapter**:
`ProviderAdapter` trait implementation. One per provider kind:
`LocalAdapter` wraps a loaded `LocalModel`; `AnthropicAdapter` wraps
the HTTP client. `ChatState` holds `Arc<dyn ProviderAdapter>`.

**Active Session**:
The currently-loaded model for chatting. In-memory in `ChatState`,
not persisted directly. Recovered on next launch via the session
resolver reading `preferences`.

**Session Resolver**:
The chain `last_active_model → default_model (device) → default_model
(vault) → first-available → onboarding` that decides which model
loads on chat-page mount. Only the terminal load runs — no
preference-write happens along this chain.

**Rückfrage** (choice prompt, spec 046):
A question the tool round puts to the user while the turn waits: either
an action that could not resolve an input (an ambiguous app name) or the
agent's own `ask_user`. The user picks a **Kandidat**, writes something
else, or declines. Shown in the chat queue next to tool approvals
(`pendingPrompts`); never times out; a cancelled turn drops it.

**Kandidat** (candidate):
One proposed answer of a Rückfrage: a label the user reads and the value
the action runs with again (for apps: title and app id). At most five per
Rückfrage; one that cannot be picked on this device is shown disabled
with its reason.

### Sprachkonventionen im UI

**Workspace / Arbeitsbereich**:
Same concept. "Arbeitsbereich" as German UI label (visible in
onboarding wizard, workspace-landing headers). "Workspace" in code,
folder names (`src/pages/workspace/`), and technical documentation.

**Standard-Modell / `default_model_id`**:
Same value. "Standard-Modell" as German UI label. `default_model_id`
as the preferences-key suffix and Rust/TypeScript identifier.

**Gerätename / `alias`**:
Same value. "Gerätename" as German UI label. `alias` as the
`known_devices` column and Rust/TypeScript identifier.

**Window Manager (`wm`) / App / Fenster (Window) / Tab / Launcher** (spec 015):
The in-app desktop concept, adopted from haex-vault 2.x (Legacy). A **Workspace**
(Arbeitsbereich) contains **Windows** (Fenster); each Window contains
one or more **Tabs**, each Tab an instance of an **App** (Chat,
Einstellungen; Föderation was an App in spec 015 and is a settings category
since spec 023). The **Launcher** lists
available Apps and opens them as Windows. "Fenster" and "Tab" as
German UI labels; `wm` as the code prefix (`useWindowManagerStore`,
`src/lib/wm/`, `src/components/wm/`, action ids `wm.*`, Tauri commands
`wm_*`) and "window"/"tab"/"app" as identifiers (`WmWindow`, `WmTab`,
`AppDefinition`). Not to be confused with the OS-level application window
(singular, Tauri-managed); the window manager is an in-app desktop rendered
inside it. Specs 015 and 020 call it **Shell**; it was renamed on
2026-09-25 because "shell" also means the command line, which agents will
be granted or denied access to (spec 021). Do not use "shell" for this
concept in new code or specs.

**Dock / Leiste / Rad** (spec 045):
The control of the workspace that holds the **Steuer-Einträge** (Launcher, window overview,
workspace overview), the apps the user **pinned** (angeheftet) and every **running** app (laufend:
at least one instance in any workspace). It shows as a **Leiste** (bar) along an edge or as a
**Rad** (wheel), one button that fans the entries out. An **Instanz** of an app is one of its tabs,
alone in its own window or next to other tabs. The entries are the vault preference `dock.items`
(synced), style and position the device preference `dock.placement`; code lives in
`src/lib/wm/dock.ts`, `useDock`, `src/components/wm/Dock*.vue`.
_Avoid_: "Taskleiste" in UI text, "Action-Menü".

**Sitzung (wm session)** (spec 022):
Which workspaces, windows and tabs are open, with window geometry and state and
each tab's location and back/forward history. Saved (`wm_sessions_no_sync`, one
JSON row per device, never synced) only while the setting "Sitzung
wiederherstellen" (`wm.session_restore`, one value for the vault since spec 023,
off by default) applies; turning it off deletes the saved session. Not the **vault
session** (spec 013: unlock to lock, one process) and not the chat's **Active
Session** (the loaded model). Avoid "Layout" for this in user-facing text.

**Ort (Location) / Tab-Historie** (spec 020):
Where a Tab stands inside its App: an app-relative path plus query
(`/thread/<id>`, `/models?sort=size`), pure data (`TabLocation`). Each
Tab keeps its own linear back/forward **Tab-Historie** (`TabHistory`),
in memory only. "Ort" and "Verlauf" as German UI wording; never the
webview's browser history, which holzi does not use as navigation state.

**Aktion (Action) / Berechtigungsbereich (Scope) / Aufrufer (Caller)**
(spec 020):
Every state-changing control of the window manager and its Apps triggers a
catalog **Aktion** (`ActionDefinition`, `runAction`) with a JSON
schema, a target, a **Berechtigungsbereich** and an effect; the
**Aufrufer** is the user, the built-in agent or an external agent.
Actions in the `guardrails` scope are user-only.
_Avoid_: "command" for these — in holzi "command" means Tauri commands.

**Werkzeug (tool) / Wirkungsart / Risikostufe** (spec 032):
A **Werkzeug** is an **Aktion** offered to a model that holzi's own chat
drives, local or reached with an API key; the tool loop lists it with source
`action`. A chat turn starts with a fixed core of tools and the search
`find_actions`; the model finds the rest and gets it from the next step on. The
**Wirkungsart** of an action (`read`, `write`, `destructive`) maps to the
**Risikostufe** the approval mode decides on: `Safe`, `Change`, `Risky`.
Actions in the `guardrails` scope are never offered. Whether a model can call
tools at all is its capability `tool_use` (`ModelCapabilities`).
_Avoid_: "command" for tools — in holzi "command" means Tauri commands.

**Passwortmanager / Kopfdaten / Verlaufsstand** (spec 034):
The built-in secret store: the app `system.passwords` of the window manager
over the tables `haex_passwords_*` of the vault, synced like all vault data.
**Kopfdaten** (`ItemHeader`) are an entry without its secrets — title,
user name, address, tags, folder, markers for a TOTP or passkeys; lists and
the search hold nothing else. A **Verlaufsstand** (`SnapshotData`) is one
saved state of an entry; restoring one makes it current and adds a state of
its own, so no state is lost. Secrets sit unencrypted in the vault database
(ADR 0007); they stay in the backend until the user asks to see or copy one.
_Avoid_: "Version" for a Verlaufsstand, "Passwort" for an entry (an entry
holds a password, a TOTP secret, custom fields, attachments and passkeys).

**Aufrufer (`Caller`) / Freigabe (`Grant`) / Bereich (`Scope`)** (spec 034):
Every access to the password manager goes through one `PasswordsService` that
takes a **Caller** — the user, the built-in agent, an extension, an external
agent, or a holzi function (`Internal`) — fixed by the entrance, never by an
argument. A **Freigabe** gives a caller `Read` or `ReadWrite` for a **Bereich**:
all entries, or those with a tag. The user needs none. The built-in agent can
only search titles, tags and folder names; no action reads a secret.
_Avoid_: confusing the **Bereich** of a Freigabe with the
**Berechtigungsbereich** (`scope`) of an action (spec 020); they are different
things that happen to share the English word.

**Ablage / Kopier-Dialog** (spec 036):
The **Ablage** is the password manager's own list of entries and folders that
were cut or copied, waiting to be pasted: ids only, shared by all its windows,
emptied when the last one closes or the vault changes. It is not the system
clipboard; nothing of an entry reaches another program that way (copying a
user name or password to the system clipboard is a separate action). Pasting
a copy opens the **Kopier-Dialog**: title or suffix, "Verlauf übernehmen",
user name or password as a Verweis on the original, "Passkeys per Verweis".
_Avoid_: "Zwischenablage" for the Ablage.

**Adressbuch / Kontakt / Angabe / Herkunft / Abgleichsziel** (spec 051):
The **Adressbuch** is the vault's own address book: the app `system.contacts`
over the tables `haex_contacts_*`, synced like all vault data and behind one
`ContactsService` with the same Callers as the password manager. A
**Kontakt** has name parts, organization, note, photo, groups and any number
of **Angaben**: rows with an open **Art** (`kind`: phone, email, address,
messenger, …, custom, holzi addresses), so later kinds need no migration and
an older device keeps and shows a kind it does not know, read-only. The
**Herkunft** says where a contact came from (made in holzi, taken over from a
phone, read from a **Visitenkarte**) and never changes. An **Abgleichsziel** is
a phone and account a contact is written to; contacts made in holzi have none
until the user picks one. **holzi-Adressen** are the vault identity, devices
and Nostr-Relays of another vault, confirmed when they come from its signed
Visitenkarte.
_Avoid_: "Kontaktbuch" (the operator says Adressbuch); "Kontakte" for the
sharing identities of haex-vault 2.x, which holzi does not have.

**Öffner / Öffner-Anmeldung / Öffnen-Auswahl** (spec 063):
An **Öffner** is an extension that takes over one kind of link, for now `mail.compose`: it
writes an e-mail. The extension registers itself in the `openers` block of its manifest
(**Öffner-Anmeldung**); holzi lists it only while it is installed and enabled, and the
registration grants no access. The **Öffnen-Auswahl** is the choice the person makes: a menu at
the mail button of an address (the system's mail program and one row per Öffner), or a dialog for
a `mailto:` link from another app (one row per Öffner and "Link kopieren", never the system).
holzi reads the link; the Öffner gets the fields (`to`, `cc`, `bcc`, `subject`, `body`) once,
through the bridge method `extension_opener_take`, never the link, and only after the person
chose it.
_Avoid_: "Handler" or "Standard-Programm" (holzi never makes itself the default for a link);
"Mail-Extension" for the registration (it is the Öffner-Anmeldung, whatever the extension is called).

**Ich-Kontakt / Weitergabe / Teilen-Dialog / Echtheitsnachweis der holzi-Adressen** (spec 060):
The **Ich-Kontakt** is the one contact of the Adressbuch that is the person
themselves: a fixed id, always there, never deleted, shown first as "Ich" whatever
name its parts give, and never offered to the phone. A **Weitergabe** is the text
`haex-vault://contact/…` of one contact: link, clipboard text and QR code are the
same text, a photo never goes along, and the receiver reads it in with a preview
and a choice per item that adds by default and never replaces a vault identity.
The **Teilen-Dialog** is the same for every contact: one row per detail, all
chosen when it opens, the choice never stored; an agent shares only through
`contacts.share`, a destructive action that asks in this dialog on every call.
The **Echtheitsnachweis der holzi-Adressen** is the signed card of a vault inside
a Weitergabe (or an older `holzi:card:` text): it proves only that the addresses
belong to that vault, not who the person is. The interface never shows the word
"Karte".
_Avoid_: "Visitenkarte" and "Karte" in what the user reads; "Export" or "vCard"
for the Weitergabe.

**Abgleich / Telefon-Adressbuch / Roh-Kontakt / Zuordnung** (spec 051 US2):
The **Abgleich** keeps the Adressbuch and the **Telefon-Adressbuch** (Android's
Contacts Provider) in step on a phone where the person switched it on, in both
directions; it runs after opening the vault, on coming back to the foreground
and when contacts change in the vault, never in the background. It writes to
the phone only contacts with that phone as their Abgleichsziel; removing the
target asks whether the phone deletes the contact or keeps it, unsynced. Android keeps a person as one
aggregated contact made of **Roh-Kontakte**, one per phone account; holzi
makes one Kontakt of them. A **Zuordnung** (`haex_contacts_phone_links`) ties
a Roh-Kontakt of one phone to a Kontakt and keeps the state both sides agreed
on, so a run compares three ways and the later change wins. Before the first
run an **Übersicht** shows what will be taken over and merged; a run that
would delete many contacts stops and asks.
_Avoid_: "Synchronisation" for the Abgleich (that is the vault's own sync between
devices, spec 024).

**Verweis (Platzhalter) / Passkey-Verbindung** (spec 036):
A **Verweis** is a placeholder in a value — `{$<id>:username}`,
`{$<id>:password}` or `{$<id>:extra:<key>}` — that stands for that value of
another entry. It is stored as written and resolved only in Rust when the
value is read, up to 12 steps; a cycle is refused on save, and an unresolved
Verweis is an error, never an empty text. A Verweis grants no access: a
caller without the source in its Bereich gets nothing. A **Passkey-Verbindung**
shows a passkey of one entry at another entry without copying its key; it
goes with the passkey or either entry.
_Avoid_: "Link" for a Verweis in the UI (it reads "Verweis auf <Quelle>"),
"Kopie" for a Passkey-Verbindung.

**Tab Details / Extra / Verlauf** (spec 036):
The three tabs of an entry in the password manager: **Details** (title, user
name, password, address, TOTP, note), **Extra** (custom fields, attachments,
passkeys) and **Verlauf** (the timeline of its Verlaufsstände). The tab is
part of the **Ort**, so a Tab-Historie and a restored Sitzung come back to it.
_Avoid_: "Tab" for these alone when the window manager's tabs could be meant;
say "Tab des Eintrags".

**Einstellungskategorie (settings category) / Unteransicht** (spec 023):
A group in the settings' sidebar — Allgemein, Darstellung, Modelle, Agenten,
Föderation (`SETTINGS_CATEGORIES` in `src/lib/settings/registry.ts`). A
category with several areas starts with an overview that leads into
**Unteransichten**; every category and sub-view is an **Ort** of the settings
tab. Settings apply to the vault on every device; only the Gerätename, the
Standard-Modell, the Spracherkennungsmodell and the Dock-Position stay per
device (FR-024). A choice saves on selection, without a save button.

**Farbschema (color scheme)** (spec 023):
Hell, Dunkel or System (`appearance.color_scheme`, vault value, default
System); System follows the operating system. Applied as the `dark` class and
`color-scheme` on `<html>`. Pages before unlocking follow the operating system.
_Avoid_: "Theme" in user-facing text.

**Geräte der Vault (vault devices)** (spec 023):
The rows of `known_devices` except the Vault Scope Sentinel, shown in the
category "Föderation" (`list_vault_devices`, action `settings.devices.list`):
this device first and marked "Dieses Gerät", the others by Gerätename, unnamed
ones as "Unbenanntes Gerät". Other devices appear once the sync exists or in a
copied vault; "zuletzt online" waits for the sync spec.

**Darstellung (appearance) / Tönung / Farbfeld / Fensterhinweis** (spec 035):
**Darstellung** is the category and the set of all look settings of a vault: the
colour scheme (hell/dunkel/automatisch) and the appearance (`appearance.theme`,
one synced value for the whole vault). A **Farbfeld** (colour field) is one
choice in a row: a preset or a custom colour (`#rrggbb`). A **Tönung** (tint)
is the hue and capped chroma of a colour field applied to the default surfaces
of the current scheme (window, container, component, text); the accent is no
tint but a colour whose lightness is derived for readability ("angepasst"
tells the user when). The **Fensterhinweis** (window hint) draws the active
window's border in the accent colour. "Sitzung" and "Window Manager (wm)" keep
their meaning; the appearance is not part of a Sitzung.

### Sync zwischen eigenen Geräten (spec 024)

**Hauptgerät (main device)**:
A device of the vault that holds the private key of the vault identity. Only
main devices sign device lists, so only they add or remove devices. A vault can
have several; its first installation is one.

**Verknüpftes Gerät (linked device)**:
A device of the vault without the private key of the vault identity. It reads
and writes all vault data but cannot add or remove devices.

**Geräteliste (device list)**:
The device list signed with the vault identity: the current devices (device
key, role, name, network id) and the removed devices with their limit, under a
generation. The valid list is the one with the highest generation; on a tie the
one with the smallest hash. Devices accept each other only when both are on it.

**Ursprungsgerät (origin device)**:
The device that wrote a change. It is the node id in the change's HLC (its
`vault_device_uuid`) and stays the same when the change is passed on.
_Avoid_: author, sender (the sender of a change can be a third device).

**Fortschrittsstand (progress)**:
Per origin device, the highest HLC this device has fully applied. Two devices
compare their progress and pull what they are missing.

**Transaktionsgruppe (transaction group)**:
All changes that share one HLC, which is one `Database::write` transaction.
It is never split and never applied in part.

**Nostr-Relay / iroh-Relay / Sync-Server**:
Three different third parties: a Nostr-Relay carries presence messages and
link rendezvous, an iroh-Relay forwards encrypted connections between devices
that cannot reach each other directly, and the Sync-Server (spec 026) keeps a
mailbox for devices that are offline.
_Avoid_: "Relay" on its own.

### Erweiterungen (spec 017)

**Erweiterung / haextension**:
An app of a third party that runs inside holzi in a tab, from a signed bundle (`.xt`, ADR-0008)
that is vault data, so every own device has it. Identified by publisher key and name; its tables
carry the prefix `<publicKey>__<name>__`.
_Avoid_: plugin, add-on.

**Prüfstelle**:
The one place in holzi that takes every request of an extension, knows which extension sent it
(from its frame, never from the request), checks the permission and runs the host function
(`extensions::bridge::dispatch`, ADR-0004 direction B).
_Avoid_: gateway, proxy.

**Erweiterungsrahmen**:
The sandboxed iframe in which an extension runs in a tab, with the channel over which it talks to
holzi. holzi creates both, so it knows whose a frame is; a frame has no access to holzi's own
interface on any platform.
_Avoid_: webview (holzi has no native extension webviews).

**Berechtigung (vault-weit / Gerät)**:
The permission of an extension to run one kind of host function on a target, in the state
granted, denied or ask. A remembered one is vault data and holds **vault-wide** (all own devices)
or for **one device** (ADR-0001); only the shell holds on one device, every other kind on all,
and nobody chooses. A temporary one
lasts until the vault closes. Spec 034 calls the same thing "Freigabe".
_Avoid_: grant for the stored row (use it only for the password manager's `Grant`).

**Erweiterungswerkzeug** (extension tool, spec 018):
A tool an extension declares in its signed manifest and the built-in agent may call. The user
confirms it when the extension is installed or updated; its effect (`read`, `change`,
`destructive`) sets how a call is approved, and an unticked tool is on "ask": every call needs the
user's consent. The model sees it as `x_<slug>_<tool>`; holzi calls it over the MCP connection of a
frame of the extension, and the history names the extension (a development version is marked as
such). Not an action of holzi's own catalog.
_Avoid_: plugin tool, skill.

**Passendes Angebot** (matched offer, spec 018):
The up to three extension tools that fit the user's message, which holzi adds to the first step's
offer so a small model does not have to search for them. Deterministic and without a model: the
words of the message against the titles, description and examples of every tool, each word
weighted by how rare it is; naming the extension offers its tools. Measured by the model
evaluation as `offered`.
_Avoid_: recommendation, suggestion.

**Mailwerkzeug** (mail tool, spec 019):
One of the fifteen **Erweiterungswerkzeuge** that haex-mail declares: accounts, folders, finding,
reading, marking, moving, drafting, sending, deleting and rules. Sending a draft, deleting for good
and creating a **Regel** are `destructive`, so they ask even in "Auto"; the consent shows only the
tool's input, so those inputs repeat what the user agrees to (recipients, subject and text of the
draft, the subject of each mail to delete) and the tool refuses when the stored mail says
otherwise. A password never passes through a mail tool: an account comes from a tagged password
manager entry or from a form the user fills in.
_Avoid_: mail action (an **Aktion** is holzi's own catalog), mail plugin.

**Regel** (mail rule, spec 019):
A lasting instruction of haex-mail: all its conditions (sender, recipient or subject contains a text,
has an attachment) hold, so its **Aktionen** run on a new message of the inbox. It covers one
account or all, is switched on or off, and is vault data that syncs to every device; it runs on a
device only while haex-mail runs there. The user makes it in the settings of haex-mail, the agent
through the tool `add_rule` (marked "created by the agent"). There is no action that forwards or
sends a message. The first look at a mailbox only sets the mark of what is new; an existing mail is
touched only when the user asks for it.
_Avoid_: filter (that is the search of `find_messages`), automation.

**Aktion (einer Regel)** (rule action, spec 019):
What a **Regel** does to a matching message: move it to a mailbox (by role, or by name for a single
account), mark it read, or flag it. A rule has at most one of each; of all matching rules the
earliest created one decides the move, and a rule whose target folder is gone is skipped as a whole.
_Avoid_: action without "rule" (see **Aktion** for the catalog of holzi).

**Modellauftrag** (model job, spec 055):
What an extension asks of holzi's language and vision models through the SDK (`models.runAsync`):
up to four steps, each with the extension's instruction, text or images or the result of an earlier
step, and the format of the answer as a schema. holzi picks a granted model per step, drops broken
output (empty, a loop, a switch of script, not the format) and tries the next granted model; only an
answer in the format is a result. A job gives its model no tools of holzi, lives only in memory and
leaves no trace in the vault; a connected subscription (Claude Code, Codex) runs as an agent with full
access, which the grant says to the user.
_Avoid_: agent task (the earlier spec 047), prompt, request.

**Modellfreigabe** (model grant, spec 055):
The **Berechtigung** of kind `model`: an extension may use this model up to this effort (`none` to
`high`). The user answers a first job's question or sets it in the extension's settings, where the
granted models also get their rank. Every model holzi knows can be granted, whatever its source. A wish
above the ceiling is capped, not refused.
_Avoid_: model permission, model access.

**Ziel (Ort)** (destination and its place, spec 055):
Where a model runs, seen from this device: `this_device` (local, or a server on loopback such as
Ollama), `network` (another device or a server in the own network) or `cloud`. Before a job sends
anything to a destination outside this device, the user consents (`modelDestination`, per provider,
for the session or remembered) and sees the provider, the address and what would be sent.
_Avoid_: location, region.

**Kanal** (channel, spec 057):
A connection between the same extension on devices of two different vaults, opened by an accepted
invitation or a join with a **Beitrittscode**. It carries fast messages (datagrams, may be lost) and
reliable ones (in order), end to end encrypted, and ends with a reason (`closed`, `lost`,
`timeout`). Only the frames of the extension it belongs to see it; nothing of it is stored.
_Avoid_: session, connection (for the extension's view), peer for a contact.

**Lobby** (spec 057):
A place a host's extension opens on its device so others can join with its **Beitrittscode**:
"nur Kontakte" admits own devices and contacts only, "öffentlich" also devices of other vaults,
marked as not verified (ADR 0016). Closing it makes the code useless; its channels stay.
_Avoid_: space, Raum, room.

**Beitrittscode** (join code, spec 057):
The opaque text `haex-vault:join:…` of an open **Lobby**: the host's address, a random lobby id and a
secret that never travels in the clear. It names no person, vault or contact.
_Avoid_: invite link, token.

**Einladungsabfrage** (invitation prompt, spec 057):
holzi's own question "[Kontakt] lädt dich zu [Erweiterung] ein" with Annehmen/Ablehnen, in the
window or as holzi's notification. Only the user answers it; the extension learns of an invitation
only after it was accepted.
_Avoid_: invitation dialog of the extension, accept call.

**Peer-Berechtigung** (spec 057):
The **Berechtigung** of kind `peers` (action `connect`, target `*`), vault-wide: the extension may
open channels to itself on devices of other vaults.
_Avoid_: network permission, peer for a contact.

**Marketplace** (spec 069):
A source of extensions that HaexVault asks over HTTPS by its address (with an optional path prefix) and the
protocol of `specs/069-marketplace/contracts/marketplace-api.md`. Every marketplace is an equal row of the
vault (name, address, optional login, position); several are used at once and their entries are never
merged, so one cannot hide the entry of another. The app that shows their catalog is called "Marktplatz".
A login is an entry of the password manager (tag `haex-marketplace`), never a value in the row.
_Avoid_: store, registry, repository, "the" marketplace for the public one.

**Voreinstellung** (preset, spec 069):
The row `preset-public` that points at the project's public marketplace. It is an ordinary marketplace:
every vault holds it (one from before the marketplace gets it when it is opened on a main device); it is
switchable and editable but not deletable, so the public marketplace is always there. No code outside `marketplace/store.rs` knows its address.
_Avoid_: default marketplace, built-in marketplace.

### Dateizugriff und Dateifreigaben (spec 056)

**Geräte-Dateizugriff** (device file access):
An own device reaching the whole file system of another own device, as that device's file browser
shows it, over `haex-vault-files/1`. It needs no **Dateifreigabe**. A switch per device, kept only
on that device (`_no_sync`, on by default), turns serving other own devices off.
_Avoid_: remote drive, mount.

**Dateifreigabe** (file share):
A folder or a single file the user makes reachable to contacts: a name, a source (the device or a
storage, and a path), the device that serves it, and **Zugriffsrechte**. Its root is all a contact
sees; nothing above or beside it.
_Avoid_: "Freigabe" alone (spec 034 Grant), "Space", "Share" in user-facing text.

**Zugriffsrecht** (share grant):
One vault identity (a contact) in a **Dateifreigabe** with the level Lesen or Lesen und Schreiben.
Taking it away rolls out no keys; the serving device stops answering that vault for that share on
its next request.
_Avoid_: permission (extension, agent), "Freigabe".

**Empfangene Dateifreigabe** (received file share):
What a recipient's vault knows of a contact's **Dateifreigabe**: the contact, name, level and the
serving device. It shows in the file browser like a storage.
_Avoid_: shared folder, mount.

**Erreichen über die Karte** (dialing from the card):
A device of a contact is reached at the endpoint id and iroh relay its stored business card names,
as in spec 057; no other address is used. A device without a grant for the asking vault closes in
the handshake.
_Avoid_: Anklopfen, knock (dropped 2026-10-23), presence (presence is for own devices).

### Sync-Regeln (spec 061)

**Sync-Regel** (sync rule):
Keeps two folders of the file browser equal, one-way (A→B) or both ways. It belongs to the device
that created it, runs only there while holzi is active, and is not synced to other devices.
_Avoid_: "Sync" alone (that is the vault's own sync between devices, spec 024), "Space".

**Ort (einer Sync-Regel)** (rule place):
A folder of a file browser source as a rule sees it: the source (`SourceRef`) and the window path,
also into an encrypted folder or another own device. Fremde Dateifreigaben and holzi's own places
are never one.
_Avoid_: "Ort" without context where the wm **Ort** (tab location, spec 020) is meant.

**Abgleich (einer Sync-Regel) / Stand / Plan** (rule run, state, plan):
One run of a rule. It compares both places with the **Stand** (size and modification time of each
file on each side after the last success) and builds a **Plan** of steps (copy, move, delete,
conflict copy) before it changes anything. Times are only compared with the Stand of the same side.
_Avoid_: mixing it up with the **Abgleich** of the address book (spec 051); say "Abgleich der Regel"
where both could be meant.

**Konfliktkopie / Löschmodus / Regel-Papierkorb** (conflict copy, delete mode, rule trash):
A file changed on both sides keeps the newer version under its name; the older one lies beside it as
`Name (Konflikt Gerätename Datum).Endung` on both sides. The **Löschmodus** (spiegeln, behalten,
nachfragen) says what a deletion on one side does to the other; mirrored deletions go to the system
trash or to the **Regel-Papierkorb** `.holzi-trash` in the place, never for good.

**Ausnahmedatei** (exclusion file):
`.haex-ignore` in the root of a place, with patterns as in `.gitignore`. A file is excluded when the
patterns of either side match it; the rules UI edits the file, which is the only source of patterns.
_Avoid_: ignore list, filter (as a stored rule setting).

**Lauf-Protokoll / Protokolltiefe** (run log, log depth):
The last runs of a rule with counters and one entry per file action. The **Protokolltiefe** (setting
"Sync-Protokoll", default 10, 0 to 100) says how many runs per rule this device keeps.

### Internationalisierung (i18n)

All user-visible text uses `@nuxtjs/i18n`. Backend commands and
events return structured data (enum values, IDs, parameters), never
localized strings. Locale files under `src/i18n/locales/de.json` and
`src/i18n/locales/en.json` — both languages MUST be maintained in lockstep
for every user-visible string introduced by a feature.
_Avoid_: hardcoded German strings in `.vue` files or in backend
event payloads.
