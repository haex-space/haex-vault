//! Renaming over an existing name (spec 050 research R3): plain SFTP rename refuses a target that
//! exists (OpenSSH), and russh-sftp does not speak `posix-rename@openssh.com`. holzi sends that
//! extension on its second channel when the server offers it; otherwise it removes the target file
//! first and renames, as rclone does. The short time without a target is the "as far as the
//! server allows" of the spec's edge case.

use russh_sftp::protocol::{Packet, StatusCode};

use super::super::StorageError;
use super::connect::{sftp_error, Session};

/// The extension that renames over an existing file.
const POSIX_RENAME: &str = "posix-rename@openssh.com";

/// `text` as an SSH string: its length, then its bytes.
fn ssh_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}

/// Renames the file `from` to `to`, replacing a file at `to`.
pub(super) async fn rename_over(
    session: &Session,
    from: &str,
    to: &str,
) -> Result<(), StorageError> {
    if session.extensions.posix_rename {
        let mut data = Vec::new();
        ssh_string(&mut data, from);
        ssh_string(&mut data, to);
        return match session.raw.extended(POSIX_RENAME, data).await {
            Ok(Packet::Status(status)) if status.status_code == StatusCode::Ok => Ok(()),
            Ok(Packet::Status(status)) => Err(sftp_error(status.into())),
            Ok(_) => Err(StorageError::Protocol),
            Err(error) => Err(sftp_error(error)),
        };
    }
    match session.sftp.remove_file(to).await.map_err(sftp_error) {
        Ok(()) | Err(StorageError::NotFound) => {}
        Err(error) => return Err(error),
    }
    session.sftp.rename(from, to).await.map_err(sftp_error)
}

/// Renames `from` (a file or a folder) to `to`, which must not exist.
pub(super) async fn rename_new(
    session: &Session,
    from: &str,
    to: &str,
) -> Result<(), StorageError> {
    session.sftp.rename(from, to).await.map_err(sftp_error)
}
