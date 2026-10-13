//! Credentials as the window typed them, checked against the protocol of their connection (spec
//! 050 contracts/tauri-commands.md): an access key for S3, a password for WebDAV and SFTP, a key or
//! a key holzi generated for SFTP. Secrets are kept exactly as entered.

use zeroize::Zeroizing;

use super::invalid;
use crate::error::Result;
use crate::remote_storage::model::CredentialsInput;
use crate::remote_storage::sftp::keys;
use crate::remote_storage::{
    Credentials, PasswordCredentials, Protocol, S3Credentials, SshKeyCredentials,
};

/// Validates required credential fields for `protocol` (spec 050 contracts/tauri-commands.md) and
/// normalizes the key id, the user name and the optional token. Secrets are kept exactly as entered.
pub(super) fn credentials_of(input: CredentialsInput, protocol: Protocol) -> Result<Credentials> {
    let nonempty = |text: &Zeroizing<String>| !text.is_empty();
    match (input, protocol) {
        (
            CredentialsInput::S3 {
                access_key_id,
                secret_access_key,
                session_token,
            },
            Protocol::S3,
        ) => {
            let access_key_id = access_key_id.trim().to_owned();
            if access_key_id.is_empty() || !nonempty(&secret_access_key) {
                return Err(invalid("credentials"));
            }
            Ok(Credentials::S3(S3Credentials {
                access_key_id,
                secret_access_key,
                session_token: session_token
                    .filter(|token| !token.is_empty())
                    .map(|token| Zeroizing::new(token.trim().to_owned())),
            }))
        }
        (CredentialsInput::Password { username, password }, Protocol::WebDav | Protocol::Sftp) => {
            let username = username.trim().to_owned();
            if username.is_empty() || !nonempty(&password) {
                return Err(invalid("login"));
            }
            Ok(Credentials::Password(PasswordCredentials {
                username,
                password,
            }))
        }
        (
            CredentialsInput::SshKey {
                username,
                private_key,
                passphrase,
            },
            Protocol::Sftp,
        ) => {
            let username = username.trim().to_owned();
            if username.is_empty() || private_key.trim().is_empty() {
                return Err(invalid("login"));
            }
            Ok(Credentials::SshKey(SshKeyCredentials {
                username,
                private_key,
                passphrase: passphrase.filter(|text| !text.is_empty()),
            }))
        }
        // A key holzi generated: its private half from the process (research R4).
        (CredentialsInput::GeneratedKey { username, key_id }, Protocol::Sftp) => {
            let username = username.trim().to_owned();
            let private_key = keys::pending()
                .get(&key_id)
                .ok_or_else(|| invalid("credentials"))?;
            if username.is_empty() {
                return Err(invalid("login"));
            }
            Ok(Credentials::SshKey(SshKeyCredentials {
                username,
                private_key,
                passphrase: None,
            }))
        }
        _ => Err(invalid("credentials")),
    }
}
