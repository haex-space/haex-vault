use super::*;
use crate::files::EncryptedInfo;

fn query(name: &str, is_dir: bool) -> IconQuery {
    IconQuery {
        name: name.to_owned(),
        is_dir,
        no_access: false,
        encrypted: None,
    }
}

fn encrypted(role: EncryptedRole, state: EncryptedState) -> Option<EncryptedInfo> {
    Some(EncryptedInfo {
        role,
        state,
        foreign: 0,
    })
}

#[test]
fn a_file_has_the_role_of_its_viewer_kind() {
    for (name, role) in [
        ("photo.PNG", IconRole::Image),
        ("clip.mp4", IconRole::Video),
        ("song.flac", IconRole::Audio),
        ("paper.pdf", IconRole::Pdf),
        ("main.rs", IconRole::Text),
        ("archive.zip", IconRole::File),
        ("noextension", IconRole::File),
    ] {
        assert_eq!(role_of(&query(name, false)), role, "{name}");
    }
}

#[test]
fn a_folder_without_access_is_its_own_role() {
    let mut folder = query("secret", true);
    assert_eq!(role_of(&folder), IconRole::Folder);
    folder.no_access = true;
    assert_eq!(role_of(&folder), IconRole::FolderNoAccess);
}

#[test]
fn the_states_of_encryption_are_roles_of_their_own() {
    let mut folder = query("abc.hxef", true);
    folder.encrypted = encrypted(EncryptedRole::Folder, EncryptedState::Readable);
    assert_eq!(role_of(&folder), IconRole::EncryptedFolder);

    for state in [EncryptedState::OtherVault, EncryptedState::NewerFormat] {
        folder.encrypted = encrypted(EncryptedRole::Folder, state);
        assert_eq!(role_of(&folder), IconRole::LockedFolder);
        let mut file = query("main.rs", false);
        file.encrypted = encrypted(EncryptedRole::Inside, state);
        assert_eq!(role_of(&file), IconRole::LockedFile, "never the text role");
    }

    for is_dir in [true, false] {
        let mut entry = query("x", is_dir);
        entry.encrypted = encrypted(EncryptedRole::Inside, EncryptedState::Damaged);
        assert_eq!(role_of(&entry), IconRole::Damaged);
    }
}

#[test]
fn a_readable_entry_inside_an_encrypted_folder_has_its_ordinary_role() {
    let mut file = query("photo.png", false);
    file.encrypted = encrypted(EncryptedRole::Inside, EncryptedState::Readable);
    assert_eq!(role_of(&file), IconRole::Image);
    let mut folder = query("docs", true);
    folder.encrypted = encrypted(EncryptedRole::Inside, EncryptedState::Readable);
    assert_eq!(role_of(&folder), IconRole::Folder);
}
