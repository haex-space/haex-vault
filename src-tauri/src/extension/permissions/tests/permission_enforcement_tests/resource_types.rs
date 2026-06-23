use super::helpers::{
    create_cloud_storage_permission, create_db_permission, create_fs_permission,
    create_web_permission,
};
use crate::extension::permissions::types::{
    DbAction, FsAction, PermissionStatus, ResourceType, RwAction,
};

#[test]
fn test_permission_resource_types() {
    // Verify that different resource types are properly distinguished
    let db_perm = create_db_permission("ext", DbAction::Read, "*", PermissionStatus::Granted);
    assert!(matches!(db_perm.resource_type, ResourceType::Db));

    let fs_perm = create_fs_permission("ext", FsAction::Read, "/path", PermissionStatus::Granted);
    assert!(matches!(fs_perm.resource_type, ResourceType::Fs));

    let web_perm = create_web_permission("ext", "https://*", PermissionStatus::Granted);
    assert!(matches!(web_perm.resource_type, ResourceType::Web));

    let cloud_storage_perm =
        create_cloud_storage_permission("ext", RwAction::Read, "*", PermissionStatus::Granted);
    assert!(matches!(
        cloud_storage_perm.resource_type,
        ResourceType::CloudStorage
    ));
}
