use crate::crdt::column_sig::preimage::push_field;

/// Domain separation tag for [`RegistryRowSigPayload::canonical_encoding`].
///
/// Distinct from `column_sig`'s `haex/space-col-sig/v1` tag so a signature
/// over a registry row can never be replayed as a signature over a column
/// value, or vice versa.
pub const DOMAIN_TAG: &str = "haex/space-registry-row/v1";

const OPTION_ABSENT: u8 = 0;
const OPTION_PRESENT: u8 = 1;

/// Push an optional text field, tagging presence so `None` and `Some("")`
/// never collide. Mirrors the storage-class tag pattern in
/// `column_sig::value_bytes::to_canonical_bytes`, scoped down to a plain
/// presence/absence tag since these fields are always text-or-absent.
fn push_optional_field(buf: &mut Vec<u8>, field: Option<&str>) {
    match field {
        None => buf.push(OPTION_ABSENT),
        Some(value) => {
            buf.push(OPTION_PRESENT);
            push_field(buf, value.as_bytes());
        }
    }
}

/// Canonical, signable identity of a `haex_shared_space_sync` registry row.
///
/// A registry row is an atomic, immutable claim ("who owns this share
/// entry") — see migration `0014_registry_authorization_schema.sql`. This
/// payload is the preimage signed by the authoring extension/device and
/// verified by the puller (later tasks); this module only builds the
/// canonical byte form.
///
/// Fields borrow (`&'a str`) rather than own, so building the preimage on
/// the sign/verify hot path allocates nothing beyond the output `Vec<u8>`.
#[derive(Debug, Clone)]
pub struct RegistryRowSigPayload<'a> {
    pub id: &'a str,
    pub space_id: &'a str,
    pub table_name: &'a str,
    pub row_pks: &'a str,
    pub extension_public_key: &'a str,
    pub extension_name: &'a str,
    pub category: Option<&'a str>,
    pub r#type: Option<&'a str>,
    pub category_label: Option<&'a str>,
    pub type_label: Option<&'a str>,
    pub authored_by_did: &'a str,
    pub created_at: &'a str,
}

impl RegistryRowSigPayload<'_> {
    /// Length-prefixed concatenation of the domain tag followed by every
    /// field in fixed order (mirrors
    /// `column_sig::preimage::build_preimage`). Optional fields carry a
    /// one-byte presence tag ahead of their length-prefixed bytes so `None`
    /// and `Some("")` encode differently.
    ///
    /// Field order: id, space_id, table_name, row_pks, extension_public_key,
    /// extension_name, category, type, category_label, type_label,
    /// authored_by_did, created_at. No field-name bytes are embedded — like
    /// `build_preimage`, field identity comes from fixed position, not from
    /// an embedded label.
    pub fn canonical_encoding(&self) -> Vec<u8> {
        // Exact size (mirrors `column_sig::preimage::build_preimage`'s
        // capacity calc): each required field costs a 4-byte length prefix
        // plus its bytes; each optional field costs a 1-byte presence tag
        // plus, when present, the same 4-byte-prefix-plus-bytes shape.
        let optional_len = |field: Option<&str>| 1 + field.map_or(0, |v| 4 + v.len());
        let mut buf = Vec::with_capacity(
            4 + DOMAIN_TAG.len()
                + 4
                + self.id.len()
                + 4
                + self.space_id.len()
                + 4
                + self.table_name.len()
                + 4
                + self.row_pks.len()
                + 4
                + self.extension_public_key.len()
                + 4
                + self.extension_name.len()
                + optional_len(self.category)
                + optional_len(self.r#type)
                + optional_len(self.category_label)
                + optional_len(self.type_label)
                + 4
                + self.authored_by_did.len()
                + 4
                + self.created_at.len(),
        );
        push_field(&mut buf, DOMAIN_TAG.as_bytes());
        push_field(&mut buf, self.id.as_bytes());
        push_field(&mut buf, self.space_id.as_bytes());
        push_field(&mut buf, self.table_name.as_bytes());
        push_field(&mut buf, self.row_pks.as_bytes());
        push_field(&mut buf, self.extension_public_key.as_bytes());
        push_field(&mut buf, self.extension_name.as_bytes());
        push_optional_field(&mut buf, self.category);
        push_optional_field(&mut buf, self.r#type);
        push_optional_field(&mut buf, self.category_label);
        push_optional_field(&mut buf, self.type_label);
        push_field(&mut buf, self.authored_by_did.as_bytes());
        push_field(&mut buf, self.created_at.as_bytes());
        buf
    }
}
