//! Identifiers of extensions and their registry rows (spec 017, data-model.md, research R5).
//!
//! The registry has no UNIQUE constraints: a UNIQUE conflict halts the sync of haex-crdt. Every
//! synced row instead gets a UUIDv5 over its natural key, so two devices that install the same
//! extension or remember the same permission produce the same row and merge.
//!
//! An extension is identified by its publisher key and name. Its tables are named
//! `<publicKey>__<name>__<table>` (the convention of haex-vault 2.x (Legacy) and the vault-sdk); [`ExtensionTable`]
//! splits such a name **exactly** into its three parts, so the prefix of one extension can never
//! cover the tables of another (haex-vault 2.x compared with `starts_with`, which let `a` own the
//! tables of `a__b`).

use std::fmt;

use uuid::Uuid;

/// Namespace of `extensions.id`. Generated once; **never change** any of these namespaces, or the
/// same extension, bundle or permission would get different ids on old and new devices.
pub const NS_EXT: Uuid = Uuid::from_u128(0x46305116_2297_412e_a6b1_0afa589e8b77);
/// Namespace of `extension_bundles.id`. Never change.
pub const NS_BUNDLE: Uuid = Uuid::from_u128(0xbad49aac_3e87_44ab_af48_5078f8d055d4);
/// Namespace of `extension_bundle_files.id`. Never change.
pub const NS_BFILE: Uuid = Uuid::from_u128(0x3d675217_6c90_4896_90fc_ac7bfd75ba2b);
/// Namespace of `extension_migrations.id`. Never change.
pub const NS_MIG: Uuid = Uuid::from_u128(0xde67adbe_900d_4593_9804_d446abcbc409);
/// Namespace of `extension_permissions.id`. Never change.
pub const NS_PERM: Uuid = Uuid::from_u128(0x52070bbb_8251_459f_8289_2309ad641897);
/// Namespace of `extension_limits.id`. Never change.
pub const NS_LIM: Uuid = Uuid::from_u128(0x4301fbb4_1b61_4014_a904_401b47498c48);
/// Namespace of `extension_model_order.id`. Never change.
pub const NS_MODEL_ORDER: Uuid = Uuid::from_u128(0x7dd2d12a_d2f4_405b_a53b_3b08271c5102);
/// Namespace of `extension_device_status.id`. Never change.
pub const NS_DEVST: Uuid = Uuid::from_u128(0x2577efb5_6122_41eb_bf81_8827fd63a8b7);
/// Namespace of `dev_extensions_no_sync.id`. Never change.
pub const NS_DEV: Uuid = Uuid::from_u128(0x91a7ced9_243f_4af6_906e_bd047acc5b5d);
/// Namespace of the vault id in an extension's area of a remote storage (spec 038, research R4).
/// Never change, or an extension would lose its objects.
pub const NS_VAULT: Uuid = Uuid::from_u128(0x552cfb7a_e8a0_431b_8536_00df6bc14b33);

/// The separator between the parts of an extension table name.
pub const TABLE_SEPARATOR: &str = "__";

/// A name or key that does not have the required form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("the publisher key must be 64 lowercase hexadecimal characters")]
    InvalidPublicKey,
    #[error("the extension name must match ^[a-z][a-z0-9-]*$")]
    InvalidName,
    #[error("not the name of an extension table")]
    InvalidTableName,
}

/// The publisher key of an extension: an Ed25519 public key as 64 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PublicKey(String);

impl PublicKey {
    /// Parses a key as it appears in a manifest: exactly 64 lowercase hex characters.
    pub fn parse(value: &str) -> Result<Self, IdError> {
        let valid = value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(IdError::InvalidPublicKey)
        }
    }

    /// Parses a key from a table name, where SQLite ignores case.
    pub fn parse_case_insensitive(value: &str) -> Result<Self, IdError> {
        Self::parse(&value.to_ascii_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The 32 bytes the hex characters stand for; `parse` admits nothing else.
    pub fn bytes(&self) -> [u8; 32] {
        let mut bytes = [0; 32];
        for (byte, pair) in bytes.iter_mut().zip(self.0.as_bytes().as_chunks::<2>().0) {
            let digit = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
            *byte = digit(pair[0]) << 4 | digit(pair[1]);
        }
        bytes
    }
}

/// The name of an extension (FR-004): a lowercase letter, then lowercase letters, digits and `-`.
/// It can never contain `__`, which keeps table prefixes unambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtensionName(String);

impl ExtensionName {
    /// The rule is the bundle format's (`haex-bundle`), so a name the tool signs is a name holzi
    /// accepts.
    pub fn parse(value: &str) -> Result<Self, IdError> {
        if haex_bundle::verify::is_valid_extension_name(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(IdError::InvalidName)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The table prefix of one extension: `<publicKey>__<name>__`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TablePrefix {
    pub public_key: PublicKey,
    pub name: ExtensionName,
}

impl fmt::Display for TablePrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{TABLE_SEPARATOR}{}{TABLE_SEPARATOR}",
            self.public_key.as_str(),
            self.name.as_str()
        )
    }
}

/// A table that belongs to an extension, split into prefix and table part. Names compare without
/// regard to case, as SQLite resolves them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtensionTable {
    pub prefix: TablePrefix,
    /// The extension's own name for the table, lower case.
    pub table: String,
}

impl ExtensionTable {
    /// Splits `name` into exactly three parts at `__`: publisher key, extension name, table.
    /// Anything else — fewer or more parts, an invalid key or name, a table part that does not
    /// start with a letter or contains other characters than letters, digits, `_` and `-` — is
    /// not an extension table.
    pub fn parse(name: &str) -> Result<Self, IdError> {
        let lower = name.to_ascii_lowercase();
        let parts: Vec<&str> = lower.split(TABLE_SEPARATOR).collect();
        let [key, extension, table] = parts.as_slice() else {
            return Err(IdError::InvalidTableName);
        };
        let public_key = PublicKey::parse(key).map_err(|_| IdError::InvalidTableName)?;
        let name = ExtensionName::parse(extension).map_err(|_| IdError::InvalidTableName)?;
        let mut chars = table.bytes();
        let table_valid = matches!(chars.next(), Some(b'a'..=b'z'))
            && chars
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
        if !table_valid {
            return Err(IdError::InvalidTableName);
        }
        Ok(Self {
            prefix: TablePrefix { public_key, name },
            table: (*table).to_owned(),
        })
    }
}

/// `extensions.id`: `UUIDv5(NS_EXT, lower(publicKey) ":" name)`.
pub fn extension_id(public_key: &PublicKey, name: &ExtensionName) -> Uuid {
    Uuid::new_v5(
        &NS_EXT,
        format!("{}:{}", public_key.as_str(), name.as_str()).as_bytes(),
    )
}

/// `extension_bundles.id`: `UUIDv5(NS_BUNDLE, sha256 of the signed message, hex)`. The same
/// signature gives the same row, so per-column last-writer-wins can never mix two bundles.
pub fn bundle_id(signed_message_sha256_hex: &str) -> Uuid {
    Uuid::new_v5(&NS_BUNDLE, signed_message_sha256_hex.as_bytes())
}

/// `extension_bundle_files.id`: `UUIDv5(NS_BFILE, bundle_id ":" path)`.
pub fn bundle_file_id(bundle_id: Uuid, path: &str) -> Uuid {
    Uuid::new_v5(&NS_BFILE, format!("{bundle_id}:{path}").as_bytes())
}

/// `extension_migrations.id`: `UUIDv5(NS_MIG, extension_id ":" name ":" sql_sha256)`. The same
/// name with other SQL gives a second row, which is detectable.
pub fn migration_id(extension_id: Uuid, name: &str, sql_sha256_hex: &str) -> Uuid {
    Uuid::new_v5(
        &NS_MIG,
        format!("{extension_id}:{name}:{sql_sha256_hex}").as_bytes(),
    )
}

/// `extension_permissions.id`: `UUIDv5(NS_PERM, extension_id | kind | action | target |
/// vault_device_uuid)`. Kind and action never contain `|`, and the device uuid at the end has a
/// fixed form, so a `|` inside the target cannot make two keys equal.
pub fn permission_id(
    extension_id: Uuid,
    kind: &str,
    action: &str,
    target: &str,
    vault_device_uuid: Uuid,
) -> Uuid {
    Uuid::new_v5(
        &NS_PERM,
        format!("{extension_id}|{kind}|{action}|{target}|{vault_device_uuid}").as_bytes(),
    )
}

/// `extension_limits.id`: `UUIDv5(NS_LIM, extension_id)`.
pub fn limits_id(extension_id: Uuid) -> Uuid {
    Uuid::new_v5(&NS_LIM, extension_id.to_string().as_bytes())
}

/// `extension_model_order.id`: `UUIDv5(NS_MODEL_ORDER, extension_id "|" model_id)`. The extension
/// id has a fixed form, so a `|` inside the model id cannot make two keys equal.
pub fn model_order_id(extension_id: Uuid, model_id: &str) -> Uuid {
    Uuid::new_v5(
        &NS_MODEL_ORDER,
        format!("{extension_id}|{model_id}").as_bytes(),
    )
}

/// `extension_device_status.id`: `UUIDv5(NS_DEVST, extension_id ":" vault_device_uuid)`.
pub fn device_status_id(extension_id: Uuid, vault_device_uuid: Uuid) -> Uuid {
    Uuid::new_v5(
        &NS_DEVST,
        format!("{extension_id}:{vault_device_uuid}").as_bytes(),
    )
}

/// `dev_extensions_no_sync.id`: `UUIDv5(NS_DEV, vault_device_uuid ":" publicKey ":" name)`.
pub fn dev_extension_id(
    vault_device_uuid: Uuid,
    public_key: &PublicKey,
    name: &ExtensionName,
) -> Uuid {
    Uuid::new_v5(
        &NS_DEV,
        format!(
            "{vault_device_uuid}:{}:{}",
            public_key.as_str(),
            name.as_str()
        )
        .as_bytes(),
    )
}

/// The vault in an extension's area of a remote storage: `UUIDv5(NS_VAULT, hex(vault pubkey))`
/// (spec 038, research R4). Derived, so the bucket does not show the vault's key.
pub fn storage_vault_id(vault_pubkey: &[u8; 32]) -> Uuid {
    Uuid::new_v5(&NS_VAULT, crate::sync::keys::hex(vault_pubkey).as_bytes())
}

#[cfg(test)]
#[path = "ids_tests.rs"]
mod tests;
