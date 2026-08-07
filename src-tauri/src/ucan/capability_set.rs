//! Orthogonal capability set for UCAN payloads.
//!
//! Replaces the hierarchical [`crate::ucan::verify::CapabilityLevel`] with a
//! per-capability set where each of `Read`, `Write`, `Invite`, `Admin` is
//! individually held or not held, and independently carries a `delegatable`
//! flag. See W4 Phase C in
//! `docs/plans/2026-07-31-shared-space-authorization-impl.md`.
//!
//! **Coexistence:** during PR-1 of Phase C, this type lives alongside
//! `CapabilityLevel`. Wire integration (payload changes) lands in a later PR
//! together with Phase D (grants + transport).
//!
//! **Canonical serde form:** a JSON array of `{"cap": ..., "delegatable": ...}`
//! entries, sorted by [`Cap`] discriminant, no duplicates. Reading is lenient
//! about input order but strict about duplicates (a repeated cap indicates
//! either a malformed producer or an attempt to attacker-control precedence).

use serde::{Deserialize, Serialize};

/// The four orthogonal space capabilities. Discriminants match the historical
/// [`crate::ucan::verify::CapabilityLevel`] numeric order so that a future
/// migration from the hierarchical to the orthogonal representation can map
/// cleanly.
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

    /// True if this set holds `cap`.
    pub fn can(&self, cap: Cap) -> bool {
        self.entries.iter().any(|e| e.cap == cap)
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
/// applies it pairwise along the `prf` chain — that integration lands with
/// C.5.
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

    pub fn build(self) -> CapabilitySet {
        // Builder path cannot produce duplicates (see `with`), so the
        // `from_entries` error is unreachable here — but going through
        // it keeps the canonical-order invariant in one place.
        CapabilitySet::from_entries(self.entries)
            .expect("builder is duplicate-free by construction")
    }
}

#[cfg(test)]
mod tests;
