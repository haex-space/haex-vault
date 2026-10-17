# Import fixtures (spec 034)

All data is invented (example.invalid addresses, placeholder names). No real account, no key
material and no encrypted blob is stored here; a scan in `tests/passwords_import.rs` fails on private
key blocks and token patterns.

- `bitwarden.json`: written by hand after the documented export layout. All five item types and an
  unknown type 99, nested folders (`Work/Email`), custom fields of every type, a deleted item with
  its folder, password history, a favourite, reprompt, a collection, extra URIs with `match`, a valid
  and an invalid TOTP.
- `bitwarden.csv`: the CSV export layout, with a note that has a line break inside quotes and two
  addresses in one cell.
- `lastpass.csv`: the LastPass CSV layout, nested `grouping` with `/` and `\`, a secure note row
  (`http://sn`) with `NoteType` and key lines, a favourite.

The KeePass database and every passkey (a key pair per algorithm) are built **at run time** by
`tests/common/kdbx_fixture.rs`; nothing of that is stored.

## haex-vault 2.x (Legacy) (spec 037)

- `legacy_haex_vault_0000_passwords.sql`: the twelve `haex_passwords_*` tables and their three unique
  indexes of haex-vault 2.x, verbatim from the pinned migration (see its header). Schema only, no data.
  The encrypted vault with its rows is built **at run time** by `tests/common/legacy_haex_vault_fixture.rs`.
