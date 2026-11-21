# Contract: files and lines

See `data-model.md` for the layout. What a reader of a log (the user, whoever helps) can rely on:

- The file is UTF-8 plain text, one record per line; a stack's following lines start with four spaces.
- The first token of every line is the time in `[YYYY-MM-DD][HH:MM:SS.mmm]`, local time.
- The origin of a line from the interface is exactly `ui`; every other origin is a Rust module path.
- No line carries a passphrase, a key, an entry's field or a file's content (FR-009). The review of research R7 also looks for
  paths of the user's own folders in messages and removes them where a file name is enough.
- The oldest lines go first when the limit is reached (FR-011); a rotation never splits a line.
- The log of a vault holds only lines from the time that vault was unlocked (FR-007, FR-008).
