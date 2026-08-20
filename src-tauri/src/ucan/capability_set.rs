//! Orthogonal per-capability set for UCAN payloads.
//!
//! Each of `Read`, `Write`, `Invite`, `Admin` is independently held or not,
//! and each held capability independently carries a `delegatable` flag. See
//! W4 Phase C in `docs/plans/2026-07-31-shared-space-authorization-impl.md`.
//!
//! **Canonical serde form:** a JSON array of `{"cap": ..., "delegatable": ...}`
//! entries, sorted by [`Cap`] discriminant, no duplicates. Reading is lenient
//! about input order but strict about duplicates (a repeated cap indicates
//! either a malformed producer or an attempt to attacker-control precedence).

use serde::{Deserialize, Serialize};

/// The four orthogonal space capabilities. Discriminant order matters for
/// canonical serde ordering — [`CapabilitySet`] entries are sorted ascending
/// by `Cap as u8`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap {
    Read = 1,
    Write = 2,
    Invite = 3,
    Admin = 4,
}

/// A single capability grant: which capability, and whether the holder may
/// delegate it to a child token.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapEntry {
    pub cap: Cap,
    #[serde(default)]
    pub delegatable: bool,
}

/// A set of held capabilities, each with its own `delegatable` flag.
///
/// Internal invariant: [`Self::entries`] is sorted ascending by
/// [`Cap`] discriminant with at most one entry per [`Cap`]. This is
/// enforced at [`Self::from_entries`] and by the deserializer.
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct CapabilitySet {
    entries: Vec<CapEntry>,
}

impl CapabilitySet {
    pub fn builder() -> CapabilitySetBuilder {
        CapabilitySetBuilder::default()
    }

    /// Construct a set with exactly one capability entry — the shape emitted
    /// by every invite-token and claim-invite path today (one UCAN per stored
    /// capability, one row per grant in `haex_ucan_tokens`).
    pub fn singleton(cap: Cap, delegatable: bool) -> Self {
        let builder = Self::builder();
        match cap {
            Cap::Read => builder.read(delegatable),
            Cap::Write => builder.write(delegatable),
            Cap::Invite => builder.invite(delegatable),
            Cap::Admin => builder.admin(delegatable),
        }
        .build()
    }

    /// The **role preset** for a requested member capability (D2) — the
    /// exact set every delegation path hands out.
    ///
    /// | requested | `Read`     | `Write`    | `Invite`  | `Admin`   |
    /// |-----------|------------|------------|-----------|-----------|
    /// | `Read`    | `false`    | —          | —         | —         |
    /// | `Write`   | `false`    | `false`    | —         | —         |
    /// | `Invite`  | **`true`** | —          | `true`    | —         |
    /// | `Admin`   | `true`     | `true`     | `true`    | **`false`** |
    ///
    /// A `—` means the capability is not held at all; every other cell is
    /// that entry's `delegatable` bit. [`Self::owner_root`] is the fifth
    /// (`owner`) row of the same table.
    ///
    /// Mirrors the TypeScript `capsFromSingle` in
    /// `src/utils/auth/ucanStore.ts`. The two tables MUST stay identical:
    /// a token minted on one side is attenuation-checked on the other, and
    /// the cross-language fixture
    /// `src-tauri/tests/fixtures/ucan_chain_vectors.json` pins them against
    /// each other.
    ///
    /// # Invariant
    ///
    /// **If a set contains [`Cap::Invite`], every other cap in that set is
    /// `delegatable: true` — except [`Cap::Admin`].**
    ///
    /// [`enforce_delegatable`] reports the *first* offender in [`Cap`]
    /// discriminant order (`Read`, `Write`, `Invite`, `Admin`). An inviter
    /// whose own `Read` were non-delegatable would therefore trip on `Read`
    /// before `Invite` is ever considered: the invite capability would be
    /// **inert** and its holder could grant nothing at all. `Admin` is the
    /// deliberate exception — holding it non-delegatably is exactly what
    /// reserves minting further admins to the space root, so a delegated
    /// admin may hand out reader/writer/inviter presets but can never create
    /// another admin.
    ///
    /// The `Read` and `Write` rows deliberately keep `Read` at
    /// `delegatable: false`, and must NOT be "fixed" to `true` for symmetry
    /// with the rows below them: neither preset carries `Invite`, so neither
    /// can ever reach a delegation boundary where the bit would be read, and
    /// least privilege is the honest default there.
    ///
    /// "An admin has all rights" lives here — the `Admin` request expands to
    /// all four caps at mint time — and never in [`Self::can`], which stays
    /// exact-match. There is no rank, hierarchy or implication between caps.
    ///
    /// **Builder footgun:** the [`CapabilitySetBuilder`] boolean is
    /// `delegatable`, and calling a method at all *grants* the cap.
    /// Withholding a cap means omitting the call, not passing `false`.
    pub fn role_preset(cap: Cap) -> Self {
        match cap {
            Cap::Read => Self::builder().read(false).build(),
            Cap::Write => Self::builder().read(false).write(false).build(),
            Cap::Invite => Self::builder().read(true).invite(true).build(),
            Cap::Admin => Self::builder()
                .read(true)
                .write(true)
                .invite(true)
                .admin(false)
                .build(),
        }
    }

    /// Union of the [`Self::role_preset`] rows for every requested
    /// capability — the set granted by an invite that names more than one
    /// (`haex_space_invites.capabilities` is a JSON array; see
    /// [`crate::space_delivery::local::leader::claim`]).
    ///
    /// `delegatable` bits are OR-ed per cap, then the invariant documented
    /// on [`Self::role_preset`] is re-applied: once the union holds
    /// [`Cap::Invite`], every non-[`Cap::Admin`] entry becomes delegatable,
    /// otherwise the added caps would be inert for delegation. Without the
    /// second step a `{Write, Invite}` invite would produce
    /// `read(true) write(false) invite(true)` and its holder could hand out
    /// a reader but not a writer.
    ///
    /// [`Cap::Read`] appears on every preset row, so the session baseline
    /// every peer needs in order to Announce is preserved by construction
    /// for any non-empty input.
    pub fn role_preset_union(caps: impl IntoIterator<Item = Cap>) -> Self {
        let mut merged: Vec<CapEntry> = Vec::new();
        for cap in caps {
            for entry in Self::role_preset(cap).entries() {
                match merged.iter_mut().find(|e| e.cap == entry.cap) {
                    Some(existing) => existing.delegatable |= entry.delegatable,
                    None => merged.push(entry.clone()),
                }
            }
        }
        if merged.iter().any(|e| e.cap == Cap::Invite) {
            for entry in merged.iter_mut().filter(|e| e.cap != Cap::Admin) {
                entry.delegatable = true;
            }
        }
        Self::from_entries(merged).expect("role presets are deduplicated on merge")
    }

    /// The `owner` row of the [`Self::role_preset`] table: all four caps,
    /// every one delegatable. This is the only set that may delegate
    /// [`Cap::Admin`], which is what makes "only the space root mints
    /// admins" true.
    ///
    /// Roots are minted in TypeScript (`createRootUcanAsync`); the backend
    /// only ever *loads* an existing root out of `haex_ucan_tokens`. The
    /// mirror lives here so the Rust side pins the complete five-row table
    /// rather than four fifths of it.
    pub fn owner_root() -> Self {
        Self::builder()
            .read(true)
            .write(true)
            .invite(true)
            .admin(true)
            .build()
    }

    /// True if this set holds `cap`.
    pub fn can(&self, cap: Cap) -> bool {
        self.entries.iter().any(|e| e.cap == cap)
    }

    /// True if this set can perform an action that either the given `cap` or
    /// [`Cap::Admin`] authorizes. Encodes the ambient "Admin acts as X" rule
    /// used at membership-change and committer-validation gates in
    /// [`crate::mls::authorization`].
    ///
    /// Under orthogonal capabilities a token carrying only [`Cap::Admin`]
    /// does NOT implicitly grant [`Cap::Invite`], so a raw `can(Invite)`
    /// alone would reject a pure-Admin holder at gates that want either
    /// bit. This method makes that acceptance explicit.
    pub fn can_or_admin(&self, cap: Cap) -> bool {
        self.can(cap) || self.can(Cap::Admin)
    }

    /// True if this set holds `cap` AND that entry is marked `delegatable`.
    pub fn is_delegatable(&self, cap: Cap) -> bool {
        self.entries
            .iter()
            .find(|e| e.cap == cap)
            .map(|e| e.delegatable)
            .unwrap_or(false)
    }

    /// Iterator over the canonical (sorted, deduplicated) entries.
    pub fn entries(&self) -> impl Iterator<Item = &CapEntry> {
        self.entries.iter()
    }

    /// Construct from an arbitrary entry list. Sorts by `Cap` and rejects
    /// duplicates. Used by the deserializer and available for callers that
    /// have already assembled entries.
    pub fn from_entries(mut entries: Vec<CapEntry>) -> Result<Self, DuplicateCapError> {
        entries.sort_by_key(|e| e.cap as u8);
        for window in entries.windows(2) {
            if window[0].cap == window[1].cap {
                return Err(DuplicateCapError(window[0].cap));
            }
        }
        Ok(Self { entries })
    }
}

/// Error emitted when [`CapabilitySet::from_entries`] receives two entries
/// for the same [`Cap`]. The canonical form has no duplicates; a repeated
/// entry is either a bug or an attempt to attacker-control precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCapError(pub Cap);

impl core::fmt::Display for DuplicateCapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "duplicate capability entry: {:?}", self.0)
    }
}

impl std::error::Error for DuplicateCapError {}

// ---------------------------------------------------------------------------
// Delegation-attenuation predicate (C.2 — wire-independent)
// ---------------------------------------------------------------------------

/// Reason a child token's capability set cannot be delegated from a parent's.
///
/// A child cap violates the delegation rule in one of two ways:
/// - [`Self::Missing`] — parent doesn't hold the cap at all.
/// - [`Self::NotDelegatable`] — parent holds the cap but with `delegatable=false`.
///
/// Reported in child-cap discriminant order so callers (and tests) see a
/// deterministic first-offender surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationError {
    Missing(Cap),
    NotDelegatable(Cap),
}

impl core::fmt::Display for DelegationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Missing(cap) => write!(f, "parent does not hold capability {cap:?}"),
            Self::NotDelegatable(cap) => {
                write!(f, "parent capability {cap:?} is not delegatable")
            }
        }
    }
}

impl std::error::Error for DelegationError {}

/// Enforce the UCAN attenuation rule for one parent → child hop:
///
/// For every cap the child holds, the parent MUST hold that same cap AND
/// have it marked `delegatable=true`. A child that adds a cap the parent
/// never had, or a cap the parent held only for its own exercise
/// (`delegatable=false`), is rejected.
///
/// Note: this function only checks *this hop*. The full UCAN chain walker
/// applies it pairwise along the `prf` chain in
/// [`crate::ucan::verify::walk_prf_chain`].
pub fn enforce_delegatable(
    parent: &CapabilitySet,
    child: &CapabilitySet,
) -> Result<(), DelegationError> {
    for entry in child.entries() {
        match parent.entries().find(|p| p.cap == entry.cap) {
            None => return Err(DelegationError::Missing(entry.cap)),
            Some(parent_entry) if !parent_entry.delegatable => {
                return Err(DelegationError::NotDelegatable(entry.cap));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Serde: canonical array form, lenient about input order, strict on duplicates
// ---------------------------------------------------------------------------

impl Serialize for CapabilitySet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.entries.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CapabilitySet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries = Vec::<CapEntry>::deserialize(deserializer)?;
        Self::from_entries(entries).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Fluent builder for [`CapabilitySet`]. Setting the same cap twice overwrites
/// the previous entry (last-wins) — the resulting set still has at most one
/// entry per cap.
#[derive(Default, Debug, Clone)]
pub struct CapabilitySetBuilder {
    entries: Vec<CapEntry>,
}

impl CapabilitySetBuilder {
    pub fn read(self, delegatable: bool) -> Self {
        self.with(Cap::Read, delegatable)
    }
    pub fn write(self, delegatable: bool) -> Self {
        self.with(Cap::Write, delegatable)
    }
    pub fn invite(self, delegatable: bool) -> Self {
        self.with(Cap::Invite, delegatable)
    }
    pub fn admin(self, delegatable: bool) -> Self {
        self.with(Cap::Admin, delegatable)
    }

    fn with(mut self, cap: Cap, delegatable: bool) -> Self {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.cap == cap) {
            existing.delegatable = delegatable;
        } else {
            self.entries.push(CapEntry { cap, delegatable });
        }
        self
    }

    pub fn build(mut self) -> CapabilitySet {
        // Builder's `with` updates in place, so duplicates are impossible by
        // construction — only sorting is left to establish the canonical
        // form. Debug-assert the invariant so a future refactor of `with`
        // can't silently break it.
        self.entries.sort_by_key(|e| e.cap as u8);
        debug_assert!(
            self.entries.windows(2).all(|w| w[0].cap != w[1].cap),
            "builder produced duplicate cap entries"
        );
        CapabilitySet {
            entries: self.entries,
        }
    }
}

// ---------------------------------------------------------------------------
// Cap parse helper (wire-boundary)
// ---------------------------------------------------------------------------

/// Reason [`cap_from_str`] could not resolve a string to a [`Cap`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCapError(pub String);

impl core::fmt::Display for ParseCapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unrecognized capability string: {:?}", self.0)
    }
}

impl std::error::Error for ParseCapError {}

/// Parse a bare cap name — `"read" | "write" | "invite" | "admin"` — into
/// a [`Cap`]. Used at the Tauri-command boundary where the frontend sends
/// a capability string that the backend must lift into the typed enum.
///
/// **Wire bridge:** callers pre-Task-8 may still emit the legacy `"space/*"`
/// prefixed form (`"space/read"`, …); this helper strips the prefix so
/// backend code can migrate to the new representation ahead of the
/// frontend. TODO(Task 8): remove the prefix bridge once the frontend
/// stops emitting it.
///
/// Case-sensitive by design — the wire format is stable lowercase.
pub fn cap_from_str(s: &str) -> Result<Cap, ParseCapError> {
    let stripped = s.strip_prefix("space/").unwrap_or(s);
    match stripped {
        "read" => Ok(Cap::Read),
        "write" => Ok(Cap::Write),
        "invite" => Ok(Cap::Invite),
        "admin" => Ok(Cap::Admin),
        _ => Err(ParseCapError(s.to_string())),
    }
}

#[cfg(test)]
mod tests;
