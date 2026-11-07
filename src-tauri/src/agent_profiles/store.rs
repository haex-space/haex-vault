//! The three tables of spec 083 (data-model.md): profiles, their rights rows and the memberships of
//! subjects, and the active profile (a vault preference). Reads take `&mut impl Query` (a transaction or the read-only view), writes take the
//! CRDT transaction, so a caller can read and write in one `write`. Deletes are explicit, so each
//! row leaves its own delete marker (the tables have no cascading foreign key).

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{grant_id, member_id, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grant_rows::{rows_of, GrantRow};
use crate::mcp_server::grants::Rights;
use crate::storage::query::Query;

/// The name stored for "Standard" when its row is first written. The pages show a translated
/// label for the profile with [`STANDARD_PROFILE_ID`] and ignore this one.
pub const STANDARD_NAME: &str = "Standard";

/// Why a write was refused.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A profile needs a name.
    #[error("the name is empty")]
    NameEmpty,
    /// "Standard" cannot be renamed or deleted (FR-001).
    #[error("the profile Standard cannot be renamed or deleted")]
    StandardFixed,
    #[error(transparent)]
    Db(#[from] haex_crdt::Error),
}

impl From<StoreError> for haex_crdt::Error {
    /// For callers inside a CRDT write that only return `haex_crdt::Result`.
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Db(error) => error,
            other => haex_crdt::Error::consumer(other.to_string()),
        }
    }
}

/// A profile row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
}

/// The memberships of a subject as authorization reads them. A row this build cannot read fails
/// closed: it is reported, never skipped, so the resolver can give no right instead of a wider one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Memberships {
    /// The profiles the subject is a member of ("Standard" is implicit and not among them).
    pub profiles: Vec<Uuid>,
    /// A row of the subject that is not what its id says or names a profile that is no UUID.
    pub unreadable: bool,
}

/// Where the active profile is kept: one vault preference, synced to every device.
pub const ACTIVE_KEY: &str = "agent_profile.active";

/// The active profile as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Active {
    /// Nothing stored: "Standard".
    Unset,
    Profile(Uuid),
    /// A value this build cannot read: fails closed.
    Unreadable,
}

fn profile_of(row: &haex_crdt::rusqlite::Row<'_>) -> haex_crdt::rusqlite::Result<Option<Profile>> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    Ok(Uuid::parse_str(&id).ok().map(|id| Profile { id, name }))
}

/// Every profile that has a row. "Standard" is in it only once it has been edited.
pub fn profiles(q: &mut impl Query) -> haex_crdt::Result<Vec<Profile>> {
    Ok(q.query_map(
        "SELECT id, name FROM haex_agent_profiles ORDER BY name COLLATE NOCASE, id",
        params![],
        profile_of,
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// The profile with `id`, if it has a row.
pub fn profile(q: &mut impl Query, id: Uuid) -> haex_crdt::Result<Option<Profile>> {
    Ok(q.query_row(
        "SELECT id, name FROM haex_agent_profiles WHERE id = ?1",
        params![id.to_string()],
        profile_of,
    )?
    .flatten())
}

fn checked_name(name: &str) -> Result<&str, StoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::NameEmpty);
    }
    Ok(name)
}

/// Creates a profile. Another profile may have the same name (FR-002); only the id identifies.
pub fn create_profile(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    name: &str,
) -> Result<(), StoreError> {
    let name = checked_name(name)?;
    tx.execute(
        "INSERT INTO haex_agent_profiles (id, name) VALUES (?1, ?2)",
        params![id.to_string(), name],
    )?;
    Ok(())
}

/// Renames a profile. "Standard" keeps its name.
pub fn rename_profile(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    name: &str,
) -> Result<(), StoreError> {
    if id == STANDARD_PROFILE_ID {
        return Err(StoreError::StandardFixed);
    }
    let name = checked_name(name)?;
    tx.execute(
        "UPDATE haex_agent_profiles SET name = ?2 WHERE id = ?1",
        params![id.to_string(), name],
    )?;
    Ok(())
}

/// Writes the row of "Standard" when it has none: its first edit materialises it.
fn ensure_standard(tx: &mut CrdtTransaction<'_>) -> haex_crdt::Result<()> {
    if profile(tx, STANDARD_PROFILE_ID)?.is_none() {
        tx.execute(
            "INSERT INTO haex_agent_profiles (id, name) VALUES (?1, ?2)",
            params![STANDARD_PROFILE_ID.to_string(), STANDARD_NAME],
        )?;
    }
    Ok(())
}

/// The rights rows of a profile, unread.
pub fn rights_rows(q: &mut impl Query, profile: Uuid) -> haex_crdt::Result<Vec<GrantRow>> {
    q.query_map(
        "SELECT grant_group, grant_key, level, scope_tags FROM haex_agent_profile_grants \
         WHERE profile_id = ?1 AND typeof(grant_group) = 'text' \
           AND typeof(grant_key) = 'text' AND typeof(level) = 'text' \
           AND (scope_tags IS NULL OR typeof(scope_tags) = 'text')",
        params![profile.to_string()],
        |row| {
            Ok(GrantRow {
                group: row.get(0)?,
                key: row.get(1)?,
                level: row.get(2)?,
                tags: row.get(3)?,
            })
        },
    )
}

/// Replaces the rights of a profile. The caller has validated them (`grants::validate`). The first
/// edit of "Standard" writes its profile row too.
pub fn set_rights(
    tx: &mut CrdtTransaction<'_>,
    profile: Uuid,
    rights: &Rights,
) -> haex_crdt::Result<()> {
    if profile == STANDARD_PROFILE_ID {
        ensure_standard(tx)?;
    }
    tx.execute(
        "DELETE FROM haex_agent_profile_grants WHERE profile_id = ?1",
        params![profile.to_string()],
    )?;
    for row in rows_of(rights) {
        tx.execute(
            "INSERT INTO haex_agent_profile_grants (id, profile_id, grant_group, grant_key, level, \
             scope_tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                grant_id(profile, &row.group, &row.key),
                profile.to_string(),
                row.group,
                row.key,
                row.level,
                row.tags,
            ],
        )?;
    }
    Ok(())
}

/// The memberships of `subject`, validated against their deterministic row ids.
pub fn memberships(q: &mut impl Query, subject: &Subject) -> haex_crdt::Result<Memberships> {
    let rows = q.query_map(
        "SELECT id, profile_id FROM haex_agent_profile_members WHERE subject = ?1",
        params![subject.key()],
        |row| Ok((row.get::<_, String>(0), row.get::<_, String>(1))),
    )?;
    let mut memberships = Memberships::default();
    for (id, profile) in rows {
        let profile = profile.ok().and_then(|text| Uuid::parse_str(&text).ok());
        match (id, profile) {
            (Ok(id), Some(profile)) if id == member_id(subject, profile) => {
                memberships.profiles.push(profile);
            }
            _ => memberships.unreadable = true,
        }
    }
    Ok(memberships)
}

/// Makes `subject` a member of `profile`; its other memberships stay (spec 083 FR-006). "Standard"
/// has no row, because every subject is in it.
pub fn add_member(
    tx: &mut CrdtTransaction<'_>,
    subject: &Subject,
    profile: Uuid,
) -> haex_crdt::Result<()> {
    if profile == STANDARD_PROFILE_ID {
        return Ok(());
    }
    let id = member_id(subject, profile);
    let exists = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_agent_profile_members WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        )?
        .unwrap_or(0);
    if exists == 0 {
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES (?1, ?2, ?3)",
            params![id, subject.key(), profile.to_string()],
        )?;
    }
    Ok(())
}

/// Removes the membership of `subject` in `profile`; the others stay.
pub fn remove_member(
    tx: &mut CrdtTransaction<'_>,
    subject: &Subject,
    profile: Uuid,
) -> haex_crdt::Result<()> {
    tx.execute(
        "DELETE FROM haex_agent_profile_members WHERE id = ?1",
        params![member_id(subject, profile)],
    )?;
    Ok(())
}

/// Removes every membership of `subject` (the agent is revoked).
pub fn remove_all_memberships(
    tx: &mut CrdtTransaction<'_>,
    subject: &Subject,
) -> haex_crdt::Result<()> {
    tx.execute(
        "DELETE FROM haex_agent_profile_members WHERE subject = ?1",
        params![subject.key()],
    )?;
    Ok(())
}

/// The subjects that are members of `profile`, as stored.
pub fn members(q: &mut impl Query, profile: Uuid) -> haex_crdt::Result<Vec<String>> {
    q.query_map(
        "SELECT subject FROM haex_agent_profile_members WHERE profile_id = ?1 ORDER BY subject",
        params![profile.to_string()],
        |row| row.get::<_, String>(0),
    )
}

/// Gives every member row the id of its subject and profile. Rows from before a subject could be a
/// member of several profiles have an id of the subject alone; each is replaced by an explicit
/// delete and an insert, so the other devices, which do the same, converge. Returns how many rows
/// were replaced; run on every open, a vault that is current has nothing to do.
pub fn rekey_members(tx: &mut CrdtTransaction<'_>) -> haex_crdt::Result<usize> {
    let rows = tx.query_map(
        "SELECT id, subject, profile_id FROM haex_agent_profile_members",
        params![],
        |row| {
            Ok((
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
            ))
        },
    )?;
    let mut replaced = 0;
    for (id, subject, profile) in rows {
        let (Ok(id), Ok(subject), Ok(profile)) = (id, subject, profile) else {
            continue;
        };
        let (Some(subject), Ok(profile)) = (Subject::parse(&subject), Uuid::parse_str(&profile))
        else {
            continue;
        };
        let wanted = member_id(&subject, profile);
        if id == wanted {
            continue;
        }
        tx.execute(
            "DELETE FROM haex_agent_profile_members WHERE id = ?1",
            params![id],
        )?;
        add_member(tx, &subject, profile)?;
        replaced += 1;
    }
    Ok(replaced)
}

/// The active profile as stored (data-model.md, "The active profile").
pub fn active(q: &mut impl Query) -> haex_crdt::Result<Active> {
    Ok(
        match crate::storage::preferences::get(
            q,
            crate::storage::preferences::PrefScope::Vault,
            ACTIVE_KEY,
        )? {
            None => Active::Unset,
            Some(text) => Uuid::parse_str(&text).map_or(Active::Unreadable, Active::Profile),
        },
    )
}

/// Makes `profile` the active one; "Standard" removes the value, which is how it is unset.
pub fn set_active(tx: &mut CrdtTransaction<'_>, profile: Uuid) -> haex_crdt::Result<()> {
    use crate::storage::preferences::{delete, insert_or_update, PrefScope};
    if profile == STANDARD_PROFILE_ID {
        delete(tx, PrefScope::Vault, ACTIVE_KEY)?;
    } else {
        insert_or_update(tx, PrefScope::Vault, ACTIVE_KEY, &profile.to_string())?;
    }
    Ok(())
}

/// Deletes a profile with its rights rows and its member rows, in one write, each an explicit
/// delete, and makes "Standard" active when it was the active one. Returns how many subjects were
/// members of it (FR-009).
pub fn delete_profile(tx: &mut CrdtTransaction<'_>, id: Uuid) -> Result<usize, StoreError> {
    if id == STANDARD_PROFILE_ID {
        return Err(StoreError::StandardFixed);
    }
    let moved = members(tx, id)?.len();
    tx.execute(
        "DELETE FROM haex_agent_profile_grants WHERE profile_id = ?1",
        params![id.to_string()],
    )?;
    tx.execute(
        "DELETE FROM haex_agent_profile_members WHERE profile_id = ?1",
        params![id.to_string()],
    )?;
    tx.execute(
        "DELETE FROM haex_agent_profiles WHERE id = ?1",
        params![id.to_string()],
    )?;
    if active(tx)? == Active::Profile(id) {
        set_active(tx, STANDARD_PROFILE_ID)?;
    }
    Ok(moved)
}
